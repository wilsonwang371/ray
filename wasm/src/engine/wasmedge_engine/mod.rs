// Copyright 2020-2023 The Ray Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use super::{WasmContext, WasmFunc};
use crate::engine::{
    Hostcalls, WasmEngine, WasmInstance, WasmModule, WasmSandbox, WasmValue, TASK_RECEIVER,
    TASK_RESULT_SENDER,
};
use crate::runtime::{common_proto::TaskType, CallOptions, InvocationSpec, RayRuntime};
use crate::util::RayLog;
use std::ops::Range;
use std::time::Duration;

use anyhow::{anyhow, Result};

use lazy_static::lazy_static;

use sha256::digest;
use shellwords::split;

use std::collections::HashMap;
use std::result::Result::Ok;
use std::sync::{Arc, Mutex, RwLock};

use tracing::{debug, error, info};

use wasmedge_macro::sys_host_function;
use wasmedge_sys::plugin::PluginManager;
use wasmedge_sys::{
    AsImport, CallingFrame, Config, Executor, FuncType, Function, ImportModule, Instance, Loader,
    Module, Store, Validator, WasiInstance, WasiModule, WasmValue as WasmEdgeWasmValue,
};
use wasmedge_types::{error::HostFuncError, ValType};

mod data;
use data::*;

use libc::c_void;

use crate::runtime as rt;

const MAX_ALLOC_TRACKING: usize = 1024 * 1024;

// macro_rules! to convert wasmedge call to our call
macro_rules! hostcall_wrapper {
    ($func_name:ident) => {
        #[sys_host_function]
        fn $func_name(
            frame: CallingFrame,
            input: Vec<WasmEdgeWasmValue>,
        ) -> Result<Vec<WasmEdgeWasmValue>, HostFuncError> {
            // convert input from wasmedge type to our type
            let mut args = vec![];
            for arg in input.iter() {
                args.push(from_wasmedge_value(arg));
            }
            // TODO: move this initialization out of the hostcall
            let mut ctx = WasmEdgeContext { frame };
            match rt::$func_name(&mut ctx, &args) {
                Ok(rets) => {
                    let mut results = vec![];
                    for ret in rets.iter() {
                        let (v, data) = to_wasmedge_value(ret);
                        match data {
                            Some(_) => {
                                RayLog::error("RayBuffer return is not supported");
                                return Err(HostFuncError::Runtime(1));
                            }
                            None => {}
                        }
                        results.push(v);
                    }
                    return Ok(results);
                }
                Err(_) => {
                    return Err(HostFuncError::Runtime(1));
                }
            }
        }
    };
}

type WasmEdgeHostCallType = fn(
    CallingFrame,
    Vec<WasmEdgeWasmValue>,
    *mut c_void,
) -> Result<Vec<WasmEdgeWasmValue>, HostFuncError>;

hostcall_wrapper!(hc_ray_log_write);
hostcall_wrapper!(hc_ray_memregion_validate);
hostcall_wrapper!(hc_ray_sleep);
hostcall_wrapper!(hc_ray_init);
hostcall_wrapper!(hc_ray_shutdown);
hostcall_wrapper!(hc_ray_get);
hostcall_wrapper!(hc_ray_put);
hostcall_wrapper!(hc_ray_call);
// track memory allocation and free
hostcall_wrapper!(hc_ray_track_malloc);
hostcall_wrapper!(hc_ray_track_free);

#[allow(dead_code)]
struct CurrentTaskInfo {
    sandbox_name: String,
    module_hash: String,
    runtime: Option<Arc<RwLock<Box<dyn RayRuntime + Send + Sync>>>>,
    allocs: Vec<Range<u32>>,
    bytes: Vec<u8>,
}

lazy_static! {
    static ref HOSTCALLS: HashMap<&'static str, WasmEdgeHostCallType> = {
        let mut m: HashMap<&str, WasmEdgeHostCallType> = HashMap::new();
        m.insert("log_write", hc_ray_log_write);
        m.insert("memregion_validate", hc_ray_memregion_validate);
        m.insert("sleep", hc_ray_sleep);
        m.insert("init", hc_ray_init);
        m.insert("shutdown", hc_ray_shutdown);
        m.insert("get", hc_ray_get);
        m.insert("put", hc_ray_put);
        m.insert("call", hc_ray_call);
        m.insert("track_malloc", hc_ray_track_malloc);
        m.insert("track_free", hc_ray_track_free);
        m
    };

    // TODO: fix this temporary hack
    static ref CURRENT_TASK: Arc<RwLock<CurrentTaskInfo>> = Arc::new(RwLock::new(CurrentTaskInfo {
        sandbox_name: "".to_string(),
        module_hash: "".to_string(),
        runtime: None,
        allocs: vec![],
        bytes: vec![],
    }));

    static ref SUBMITTED_MODULES: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
}

pub struct WasmEdgeEngine {
    cfg: Config,
    args: Option<Vec<String>>,
    sandboxes: HashMap<String, WasmEdgeSandbox>,
    modules: HashMap<String, WasmEdgeModule>,
    imports: HashMap<String, ImportModule<WasmEdgeHostcallData>>,
    wasi: WasiInstance,
    plugins: HashMap<String, Instance>,
    hostcalls_map: HashMap<String, Hostcalls>,
}

impl WasmEdgeEngine {
    pub fn new(cmdline: Option<&str>, dirs: Vec<&str>) -> Self {
        let cfg = match Config::create() {
            Ok(mut cfg) => {
                cfg.wasi(true);
                cfg
            }
            Err(e) => {
                error!("Failed to create config: {:?}", e);
                panic!();
            }
        };

        let mut parsed_args = match cmdline {
            Some(cmdline) => {
                // parse cmdline
                match split(cmdline) {
                    Ok(args) => {
                        // add a first argument as the program name
                        let mut new_args = vec!["_".to_string()];
                        new_args.extend(args);
                        Some(new_args)
                    }
                    Err(e) => {
                        error!("Failed to split cmdline: {:?}", e);
                        None
                    }
                }
            }
            None => None,
        };

        match WasiModule::create(None, None, None) {
            Ok(mut wasi) => {
                let mut args = match parsed_args.as_mut() {
                    Some(args) => {
                        // convert string vec to str vec
                        let mut new_args = vec![];
                        for arg in args.iter() {
                            new_args.push(arg.as_str());
                        }
                        Some(new_args)
                    }
                    None => None,
                };

                let mut preopens = match dirs.len() {
                    0 => None,
                    _ => Some(dirs),
                };

                debug!("preopen dirs: {:?}", preopens);

                wasi.init_wasi(args, None, preopens);
                let engine = WasmEdgeEngine {
                    cfg,
                    args: parsed_args,
                    sandboxes: HashMap::new(),
                    modules: HashMap::new(),
                    imports: HashMap::new(),
                    wasi: WasiInstance::Wasi(wasi),
                    plugins: HashMap::new(),
                    hostcalls_map: HashMap::new(),
                };
                engine
            }
            Err(e) => {
                error!("Failed to create wasi: {:?}", e);
                panic!();
            }
        }
    }
}

impl WasmEngine for WasmEdgeEngine {
    fn init(&self) -> Result<()> {
        Ok(())
    }

    fn compile(&mut self, name: &str, wasm_bytes: &[u8]) -> Result<Box<&dyn WasmModule>> {
        let module = WasmEdgeModule::new(&self.cfg, name, wasm_bytes);
        self.modules.insert(name.to_string(), module);
        Ok(Box::new(&self.modules[name]))
    }

    fn create_sandbox(&mut self, name: &str) -> Result<Box<&dyn WasmSandbox>> {
        let sandbox = WasmEdgeSandbox::new(self.cfg.clone(), name);
        self.sandboxes.insert(name.to_string(), sandbox);
        Ok(Box::new(&self.sandboxes[name]))
    }

    fn register_hostcalls(&mut self, hostcalls: &Hostcalls) -> Result<()> {
        // add to hostcalls list
        self.hostcalls_map
            .insert(hostcalls.module_name.clone(), hostcalls.clone());

        Ok(())
    }

    fn instantiate(
        &mut self,
        sandbox_name: &str,
        module_name: &str,
        instance_name: &str,
    ) -> Result<Box<&dyn WasmInstance>> {
        let mut sandbox = self.sandboxes[sandbox_name].clone();

        // iterate hostcalls_map and register all hostcalls
        for (name, hostcalls) in self.hostcalls_map.iter() {
            if self.imports.contains_key(name) {
                continue;
            }
            debug!("creating import module {} from hostcalls", name);

            let hc_module_name = hostcalls.module_name.clone();
            let mut import_module = ImportModule::create(hc_module_name.as_str(), None)?;

            // This is a hack to set the current runtime
            let tmp = Arc::clone(&hostcalls.runtime);
            let mut current_task = CURRENT_TASK.write().unwrap();
            current_task.runtime = Some(tmp);
            current_task.allocs = vec![];

            for hostcall in hostcalls.functions.iter() {
                let params = &hostcall.params;
                let results = &hostcall.results;
                let func_ty = FuncType::create(
                    params
                        .iter()
                        .map(|x| wasmtype_to_wasmedgetype(x))
                        .collect::<Vec<ValType>>(),
                    results
                        .iter()
                        .map(|x| wasmtype_to_wasmedgetype(x))
                        .collect::<Vec<ValType>>(),
                )?;

                // check if hostcall is supported
                if !HOSTCALLS.contains_key(hostcall.name.as_str()) {
                    return Err(anyhow!("Hostcall not supported"));
                }
                let supported_call = HOSTCALLS[hostcall.name.as_str()];

                let func = match Function::create_sync_func::<WasmEdgeHostcallData>(
                    &func_ty,
                    Box::new(supported_call),
                    None,
                    0,
                ) {
                    Ok(f) => f,
                    Err(e) => {
                        return Err(anyhow!("Failed to create function: {:?}", e));
                    }
                };
                import_module.add_func(hostcall.name.as_str(), func);
            }
            self.imports
                .insert(hostcalls.module_name.clone(), import_module);
        }

        // register all import objects
        for (import_module_name, import_mod) in self.imports.iter() {
            let import_mod = import_mod.clone();
            match sandbox
                .executor
                .register_import_module(&mut sandbox.store, &import_mod)
            {
                Ok(_) => {
                    RayLog::info(
                        format!("Registered import module: {}", import_module_name).as_str(),
                    );
                }
                Err(e) => {
                    return Err(anyhow!("Failed to register import module: {:?}", e));
                }
            }
        }

        // register wasi instance
        match sandbox
            .executor
            .register_wasi_instance(&mut sandbox.store, &self.wasi)
        {
            Ok(_) => {
                RayLog::info("Registered wasi instance");
            }
            Err(e) => {
                return Err(anyhow!("Failed to register wasi instance: {:?}", e));
            }
        }

        // load wasi-nn
        PluginManager::load_plugins_from_default_paths();

        let mut i = match PluginManager::find("wasi_nn") {
            Some(p) => p.mod_instance("wasi_nn").unwrap(),
            None => {
                error!("Failed to find wasi_nn plugin");
                panic!();
            }
        };
        self.plugins.insert("wasi_nn".to_string(), i.clone());

        sandbox
            .executor
            .register_plugin_instance(&mut sandbox.store, &mut i)
            .unwrap();
        // sandbox.store.module_names().unwrap().iter().for_each(|x| {
        //     info!("Module: {:?}", x);
        //     sandbox
        //         .store
        //         .module(x)
        //         .unwrap()
        //         .func_names()
        //         .unwrap()
        //         .iter()
        //         .for_each(|y| {
        //             info!("   Function: {:?}", y);
        //         });
        // });

        let module = &self.modules[module_name];
        match CURRENT_TASK.write() {
            Ok(mut current_task) => {
                current_task.bytes = module.bytes.clone();
                current_task.sandbox_name = sandbox_name.to_string();
            }
            Err(e) => {
                error!("Failed to write current task: {:?}", e);
                panic!();
            }
        }

        // create instance
        let module = &self.modules[module_name].module;
        match sandbox
            .executor
            .register_active_module(&mut sandbox.store, module)
        {
            Ok(instance) => {
                let wi = WasmEdgeInstance::new(Some(instance));
                self.sandboxes
                    .get_mut(sandbox_name)
                    .unwrap()
                    .instances
                    .insert(instance_name.to_string(), wi);
                RayLog::info(
                    format!("Registered instance: {} {}", sandbox_name, instance_name).as_str(),
                );
                Ok(Box::new(
                    &self.sandboxes[sandbox_name].instances[instance_name],
                ))
            }
            Err(e) => Err(anyhow!("Failed to register module: {:?}", e)),
        }
    }

    fn execute(
        &mut self,
        sandbox_name: &str,
        instance_name: &str,
        func_name: &str,
        args: Vec<WasmValue>,
    ) -> Result<Vec<WasmValue>> {
        let sandbox = match self.sandboxes.get_mut(sandbox_name) {
            Some(s) => s,
            None => {
                RayLog::error(format!("Failed to get sandbox: {}", sandbox_name).as_str());
                return Err(anyhow!("Failed to get sandbox: {}", sandbox_name));
            }
        };

        let instance = sandbox.instances.get(instance_name).unwrap();

        let mut allocated_mem_list = vec![];

        let args = args
            .iter()
            .map(|arg| match to_wasmedge_value(arg) {
                (v, None) => Ok(v),
                (_, Some(data)) => {
                    // save data to sandbox and use the pointer as argument
                    let inner_instance = instance.inner.as_ref().unwrap();
                    match inner_instance.get_func("__war_malloc") {
                        Ok(func) => {
                            match sandbox.executor.call_func(
                                &func,
                                vec![WasmEdgeWasmValue::from_i32(data.len() as i32)],
                            ) {
                                Ok(rtn) => match rtn.len() {
                                    1 => match rtn[0].to_i32() {
                                        0 => {
                                            return Err(anyhow!("Failed to allocate memory"));
                                        }
                                        ptr => {
                                            match inner_instance.get_memory("memory") {
                                                Ok(mut mem) => {
                                                    // add the new allocated memory into the list
                                                    // so that we can free it later
                                                    allocated_mem_list.push(ptr);

                                                    // copy data to wasm memory so that wasm function
                                                    // can access it
                                                    match mem.set_data(data.as_ref(), ptr as u32) {
                                                        Ok(_) => {
                                                            return Ok(
                                                                WasmEdgeWasmValue::from_i32(ptr),
                                                            );
                                                        }
                                                        Err(_) => {
                                                            return Err(anyhow!(
                                                                "Failed to set memory"
                                                            ));
                                                        }
                                                    }
                                                }
                                                Err(_) => {
                                                    return Err(anyhow!("Failed to get memory"));
                                                }
                                            }
                                        }
                                    },
                                    _ => {
                                        return Err(anyhow!("Invalid return value"));
                                    }
                                },
                                Err(e) => {
                                    return Err(anyhow!("Failed to allocate memory: {:?}", e));
                                }
                            }
                        }
                        Err(_) => {
                            return Err(anyhow!("Failed to allocate memory"));
                        }
                    }
                }
            })
            .collect::<Result<Vec<WasmEdgeWasmValue>>>();

        let args = match args {
            Ok(args) => args,
            Err(e) => {
                return Err(anyhow!("Failed to convert arguments: {:?}", e));
            }
        };

        let res = match func_name.parse::<u32>() {
            Ok(idx) => match instance
                .inner
                .as_ref()
                .unwrap()
                .get_table("__indirect_function_table")
            {
                Ok(table) => match table.get_data(idx) {
                    Ok(val) => match val.func_ref() {
                        Some(func) => match sandbox.executor.call_func_ref(&func, args) {
                            Ok(rtn) => {
                                let results = rtn
                                    .iter()
                                    .map(|x| from_wasmedge_value(x))
                                    .collect::<Vec<WasmValue>>();
                                // TODO: remember to free allocated variable
                                Ok(results)
                            }
                            Err(e) => {
                                info!("Failed to run function: {:?}", e);
                                Err(anyhow!("Failed to run function"))
                            }
                        },
                        None => Err(anyhow!("Failed to get function")),
                    },
                    Err(_) => Err(anyhow!("Failed to get function")),
                },
                Err(_) => Err(anyhow!("Failed to get table")),
            },
            Err(_) => {
                let func = instance
                    .inner
                    .as_ref()
                    .unwrap()
                    .get_func(func_name)
                    .expect("Failed to get function");
                RayLog::info(format!("Executing function \"{}\"", func_name).as_str());
                match sandbox.executor.call_func(&func, args) {
                    Ok(rtn) => match rtn.len() {
                        0 => {
                            RayLog::info(
                                format!("Finished executing function \"{}\"", func_name).as_str(),
                            );
                            Ok(vec![])
                        }
                        1 => {
                            let val = rtn[0].to_i32();
                            if val != 0 {
                                return Err(anyhow!("Failed to run function"));
                            }
                            RayLog::info(
                                format!("Finished executing function \"{}\"", func_name).as_str(),
                            );
                            Ok(vec![WasmValue::I32(val)])
                        }
                        _ => {
                            info!("Invalid return value. {}", rtn.len());
                            Err(anyhow!("Invalid return value"))
                        }
                    },
                    Err(e) => {
                        info!("Failed to run function: {:?}", e);
                        Err(anyhow!("Failed to run function"))
                    }
                }
            }
        };

        // free allocated memory
        for ptr in allocated_mem_list.iter() {
            match instance.inner.as_ref().unwrap().get_func("__war_free") {
                Ok(func) => {
                    match sandbox
                        .executor
                        .call_func(&func, vec![WasmEdgeWasmValue::from_i32(*ptr as i32)])
                    {
                        Ok(_) => {}
                        Err(e) => {
                            return Err(anyhow!("Failed to free memory: {:?}", e));
                        }
                    }
                }
                Err(_) => {
                    return Err(anyhow!("Cannot find __war_free"));
                }
            }
        }

        res
    }

    fn has_instance(&self, sandbox_name: &str, instance_name: &str) -> Result<bool> {
        return Ok(self
            .sandboxes
            .get(sandbox_name)
            .unwrap()
            .instances
            .contains_key(instance_name));
    }

    fn has_module(&self, name: &str) -> Result<bool> {
        return Ok(self.modules.contains_key(name));
    }

    fn has_sandbox(&self, name: &str) -> Result<bool> {
        return Ok(self.sandboxes.contains_key(name));
    }

    fn list_modules(&self) -> Result<Vec<Box<&dyn WasmModule>>> {
        return Ok(self
            .modules
            .values()
            .map(|m| Box::new(m as &dyn WasmModule) as Box<&dyn WasmModule>)
            .collect());
    }

    fn list_sandboxes(&self) -> Result<Vec<Box<&dyn WasmSandbox>>> {
        return Ok(self
            .sandboxes
            .values()
            .map(|s| Box::new(s as &dyn WasmSandbox) as Box<&dyn WasmSandbox>)
            .collect());
    }

    fn list_instances(&self, sandbox_name: &str) -> Result<Vec<Box<&dyn WasmInstance>>> {
        return Ok(self.sandboxes[sandbox_name]
            .instances
            .values()
            .map(|i| Box::new(i as &dyn WasmInstance) as Box<&dyn WasmInstance>)
            .collect());
    }

    fn task_loop_once(
        &mut self,
        rt: &Arc<RwLock<Box<dyn RayRuntime + Send + Sync>>>,
    ) -> Result<()> {
        let mut result_buf: Option<Vec<u8>> = None;

        // receive task from channel with a timeout
        match TASK_RECEIVER.lock().ok().unwrap().as_ref() {
            Some(rx) => match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(task) => {
                    // RayLog::info(
                    //     format!("task_loop_once: executing wasm task: {:#x?}", task).as_str(),
                    // );
                    let mod_name = task.module_name.as_str();
                    let func_name = task.func_name.as_str();
                    let args = task.args;

                    // check if module is already compiled
                    if !self.has_module(mod_name).unwrap() {
                        RayLog::info(
                            format!("task_loop_once: compiling wasm module: {:?}", mod_name)
                                .as_str(),
                        );
                        match rt.as_ref().read().unwrap().kv_get("", mod_name) {
                            Ok(obj) => match self.compile(mod_name, &obj.as_slice()) {
                                Ok(_) => {}
                                Err(e) => {
                                    RayLog::error(
                                        format!("task_loop_once: wasm compile error: {:?}", e)
                                            .as_str(),
                                    );
                                }
                            },
                            Err(e) => {
                                RayLog::error(
                                    format!("task_loop_once: wasm compile error: {:?}", e).as_str(),
                                );
                            }
                        }
                        match self.create_sandbox("sandbox") {
                            Ok(_) => {}
                            Err(e) => {
                                RayLog::error(
                                    format!("task_loop_once: wasm sandbox error: {:?}", e).as_str(),
                                );
                            }
                        }
                        match self.instantiate("sandbox", mod_name, "instance") {
                            Ok(_) => {}
                            Err(e) => {
                                RayLog::error(
                                    format!("task_loop_once: wasm instantiate error: {:?}", e)
                                        .as_str(),
                                );
                            }
                        }
                    }

                    RayLog::info("before function execution");
                    match self.execute("sandbox", "instance", func_name, args) {
                        Ok(results) => {
                            RayLog::info(
                                format!("task_loop_once: wasm task result: {:#x?}", results)
                                    .as_str(),
                            );

                            if results.len() == 0 {
                                // if no value is returned, we return a space
                                result_buf = Some(WasmValue::msgpack_nil_vec());
                            } else if results.len() == 1 {
                                match results[0].as_msgpack_vec() {
                                    Ok(v) => {
                                        result_buf = Some(v);
                                    }
                                    Err(e) => {
                                        RayLog::error(
                                            format!("task_loop_once: wasm task error: {:?}", e)
                                                .as_str(),
                                        );
                                    }
                                }
                            } else {
                                RayLog::error("task_loop_once: invalid length of result");
                            }
                        }
                        Err(e) => {
                            RayLog::error(
                                format!("task_loop_once: wasm task error: {:?}", e).as_str(),
                            );
                        }
                    }
                }
                Err(_) => {
                    return Ok(()); // timeout
                }
            },
            None => {
                RayLog::error("task_loop_once: channel not initialized");
                return Err(anyhow!("task_loop_once: channel not initialized"));
            }
        }

        // we got result here
        match TASK_RESULT_SENDER.lock().ok().unwrap().as_ref() {
            Some(tx) => match result_buf {
                Some(buf) => {
                    RayLog::info(
                        format!("task_loop_once: sending wasm task result: {:#x?}", buf).as_str(),
                    );
                    tx.send(Ok(buf)).unwrap();
                }
                None => {
                    tx.send(Err(anyhow!("Invalid result"))).unwrap();
                }
            },
            None => {
                RayLog::error("task_loop_once: channel not initialized");
                return Err(anyhow!("task_loop_once: channel not initialized"));
            }
        }
        Ok(())
    }
}

#[allow(dead_code)]
#[derive(Clone)]
struct WasmEdgeModule {
    name: String,
    module: Module,
    bytes: Vec<u8>,
}

impl WasmEdgeModule {
    pub fn new(cfg: &Config, module_name: &str, wasm_bytes: &[u8]) -> Self {
        let loader = Loader::create(Some(&cfg)).unwrap();
        let module = loader.from_bytes(wasm_bytes).unwrap();
        match Validator::create(Some(&cfg)) {
            Ok(validator) => {
                validator.validate(&module).unwrap();
            }
            Err(e) => {
                error!("Failed to create validator: {:?}", e);
                panic!();
            }
        }
        WasmEdgeModule {
            name: module_name.to_string(),
            module,
            bytes: wasm_bytes.to_vec(),
        }
    }
}

impl WasmModule for WasmEdgeModule {}

#[allow(dead_code)]
#[derive(Clone)]
struct WasmEdgeSandbox {
    name: String,
    store: Store,
    executor: Executor,
    instances: HashMap<String, WasmEdgeInstance>,
}

impl WasmEdgeSandbox {
    pub fn new(cfg: Config, name: &str) -> Self {
        match Store::create() {
            Ok(mut store) => WasmEdgeSandbox {
                name: name.to_string(),
                store,
                executor: Executor::create(Some(&cfg), None).unwrap(),
                instances: HashMap::new(),
            },
            Err(e) => {
                error!("Failed to create store: {:?}", e);
                panic!();
            }
        }
    }
}

impl WasmSandbox for WasmEdgeSandbox {
    // TODO: implement WasmSandbox
}

#[derive(Clone)]
struct WasmEdgeInstance {
    inner: Option<Instance>,
    malloc_list: Arc<RwLock<HashMap<u32, u32>>>,
}

impl WasmEdgeInstance {
    pub fn new(instance: Option<Instance>) -> Self {
        WasmEdgeInstance {
            inner: instance,
            malloc_list: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl WasmInstance for WasmEdgeInstance {
    // TODO: implement WasmInstance
}

#[derive(Clone)]
struct WasmEdgeHostcallData {}

impl WasmEdgeHostcallData {}

struct WasmEdgeContext {
    frame: CallingFrame,
}

impl WasmEdgeContext {}

impl WasmContext for WasmEdgeContext {
    fn get_memory_region(&mut self, off: usize, len: usize) -> Result<&[u8]> {
        match self
            .frame
            .memory_mut(0)
            .unwrap()
            .data_pointer(off as u32, len as u32)
        {
            Ok(data) => Ok(unsafe { std::slice::from_raw_parts(data, len) }),
            Err(_) => Err(anyhow!("Failed to get memory region")),
        }
    }

    fn get_memory_region_mut(&mut self, off: usize, len: usize) -> Result<&mut [u8]> {
        match self
            .frame
            .memory_mut(0)
            .unwrap()
            .data_pointer_mut(off as u32, len as u32)
        {
            Ok(data) => Ok(unsafe { std::slice::from_raw_parts_mut(data, len) }),
            Err(_) => Err(anyhow!("Failed to get memory region")),
        }
    }

    fn get_func_ref(&mut self, func_idx: u32) -> Result<super::WasmFunc> {
        let module = &self.frame.module_instance().unwrap();
        let tbl = module.get_table("__indirect_function_table").unwrap();
        let func = tbl.get_data(func_idx).unwrap().func_ref().unwrap();
        let func_ty = func.ty().unwrap();
        let params = func_ty
            .params_type_iter()
            .map(|x| wasmedgetype_to_wasmtype(&x))
            .collect();
        let results = func_ty
            .returns_type_iter()
            .map(|x| wasmedgetype_to_wasmtype(&x))
            .collect();

        let sb = CURRENT_TASK.read().unwrap().sandbox_name.clone();
        let hash = match SUBMITTED_MODULES.lock().unwrap().get(&sb) {
            Some(bytes) => bytes.clone(),
            None => return Err(anyhow!("Module is not submitted")),
        };

        Ok(WasmFunc {
            sandbox: sb,
            module: hash,
            name: format!("{}", func_idx),
            params,
            results,
        })
    }

    fn invoke(
        &mut self,
        remote_func_holder: &crate::runtime::RemoteFunctionHolder,
        args: &[WasmValue],
    ) -> Result<Vec<crate::runtime::ObjectID>> {
        let call_opt = CallOptions::new();
        let invoke_spec =
            InvocationSpec::new(TaskType::NormalTask, remote_func_holder.clone(), args, None);
        let rt = &CURRENT_TASK.write().unwrap().runtime;
        match rt {
            Some(rt) => {
                let rt = rt.write().unwrap();
                match rt.call(&invoke_spec, &call_opt) {
                    Ok(results) => {
                        if results.len() != 1 {
                            return Err(anyhow!("Invalid result length"));
                        }
                        Ok(results)
                    }
                    Err(_) => Err(anyhow!("Failed to call")),
                }
            }
            None => Err(anyhow!("No runtime")),
        }
    }

    fn get_object(&mut self, object_id: &crate::runtime::ObjectID) -> Result<Vec<u8>> {
        let rt = &CURRENT_TASK.write().unwrap().runtime;
        match rt.as_ref() {
            Some(rt) => {
                let rt = rt.write().unwrap();
                match rt.get(object_id) {
                    Ok(object) => {
                        debug!("get_object: {:x?} {:x?}", object_id, object);
                        Ok(object)
                    }
                    Err(_) => Err(anyhow!("Failed to get object")),
                }
            }
            None => Err(anyhow!("No runtime")),
        }
    }

    fn put_object(&mut self, data: &[u8]) -> Result<crate::runtime::ObjectID> {
        let rt = &CURRENT_TASK.write().unwrap().runtime;
        match rt.as_ref() {
            Some(rt) => {
                let mut rt = rt.write().unwrap();
                match rt.put(data.to_vec()) {
                    Ok(object_id) => Ok(object_id),
                    Err(_) => Err(anyhow!("Failed to put object")),
                }
            }
            None => Err(anyhow!("No runtime")),
        }
    }

    fn submit_sandbox_binary(&mut self) -> Result<()> {
        let bytes = match &CURRENT_TASK.read() {
            Ok(current_task) => current_task.bytes.clone(),
            Err(_) => return Err(anyhow!("Failed to read current task")),
        };
        if bytes.len() == 0 {
            return Err(anyhow!("No module bytes"));
        }

        let sandbox_name = match &CURRENT_TASK.read() {
            Ok(current_task) => current_task.sandbox_name.clone(),
            Err(_) => return Err(anyhow!("Failed to read current task")),
        };
        let hash = digest(bytes.as_slice());
        let rt = &CURRENT_TASK.write().unwrap().runtime;
        match rt.as_ref() {
            Some(rt) => {
                let rt = rt.write().unwrap();
                match rt.kv_put("", hash.as_str(), bytes.as_slice()) {
                    Ok(_) => {
                        SUBMITTED_MODULES
                            .lock()
                            .unwrap()
                            .insert(sandbox_name.clone(), hash);
                        Ok(())
                    }
                    Err(_) => Err(anyhow!("Failed to put module bytes")),
                }
            }
            None => Err(anyhow!("No runtime")),
        }
    }

    fn track_mem_ops(&mut self, mem_ptr: u32, mem_size: usize, is_free: bool) -> Result<()> {
        match CURRENT_TASK.write() {
            Ok(mut current_task) => {
                if is_free {
                    if current_task.allocs.len() == 0 {
                        return Err(anyhow!("Memory not allocated"));
                    }
                    // remove from allocs, searching using binary search.
                    // return error if entry not found
                    let mut left = 0 as i32;
                    let mut right = (current_task.allocs.len() - 1) as i32;
                    while left <= right {
                        let mid = (left + right) / 2;
                        let alloc = &current_task.allocs[mid as usize];
                        if alloc.start == mem_ptr {
                            current_task.allocs.remove(mid as usize);
                            return Ok(());
                        } else if alloc.start > mem_ptr {
                            right = mid - 1;
                        } else {
                            left = mid + 1;
                        }
                    }
                    return Err(anyhow!("Memory not allocated"));
                } else {
                    if current_task.allocs.len() == 0 {
                        current_task.allocs.push(Range {
                            start: mem_ptr,
                            end: mem_ptr + mem_size as u32,
                        });
                        return Ok(());
                    }
                    // add to allocs to sorted list, searching using binary search.
                    // error if exists
                    let mut left = 0 as i32;
                    let mut right = (current_task.allocs.len() - 1) as i32;
                    while left <= right {
                        let mid = (left + right) / 2;
                        let alloc = &current_task.allocs[mid as usize];
                        if alloc.start == mem_ptr {
                            return Err(anyhow!("Memory already allocated"));
                        } else if alloc.start > mem_ptr {
                            right = mid - 1;
                        } else {
                            left = mid + 1;
                        }
                    }
                    current_task.allocs.insert(
                        left as usize,
                        Range {
                            start: mem_ptr,
                            end: mem_ptr + mem_size as u32,
                        },
                    );
                    return Ok(());
                }
            }
            Err(e) => {
                error!("Failed to write current task: {:?}", e);
                panic!();
            }
        };
    }

    fn lookup_mem_alloc(&mut self, mem_ptr: u32) -> Result<usize> {
        match CURRENT_TASK.read() {
            Ok(current_task) => {
                if current_task.allocs.len() == 0 {
                    return Err(anyhow!("Memory not allocated"));
                }
                // find the alloc that contains mem_ptr using binary search
                let mut left = 0 as i32;
                let mut right = (current_task.allocs.len() - 1) as i32;
                while left <= right {
                    let mid = (left + right) / 2;
                    let alloc = &current_task.allocs[mid as usize];
                    if alloc.start <= mem_ptr && mem_ptr < alloc.end {
                        return Ok((alloc.end - alloc.start) as usize);
                    } else if alloc.start > mem_ptr {
                        right = mid - 1;
                    } else {
                        left = mid + 1;
                    }
                }
                Err(anyhow!("Failed to find alloc"))
            }
            Err(e) => {
                error!("Failed to read current task: {:?}", e);
                panic!();
            }
        }
    }
}
