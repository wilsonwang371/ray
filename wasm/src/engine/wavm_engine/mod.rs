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

mod api;
use api::*;

use crate::engine::WasmType;
use crate::engine::{Hostcalls, WasmEngine, WasmInstance, WasmModule, WasmSandbox, WasmValue};
use crate::runtime::RayRuntime;
use crate::util::RayLog;
use anyhow::{anyhow, Ok, Result};
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{info, warn, debug};

use libc::c_void;

use super::{Hostcall, HostcallFunc};

type WasmHostCallCPtr = extern "C" fn(*const u128, *mut u128) -> *mut c_void;

lazy_static! {
    static ref WRAPPER_TO_HOSTCALL: RwLock<HashMap<WasmHostCallCPtr, Hostcall>> =
        RwLock::new(HashMap::new());
}

pub extern "C" fn hc_sleep_wrapper(args: *const u128, results: *mut u128) -> *mut c_void {
    info!("hc_sleep_wrapper called");
    std::ptr::null_mut()
}

pub extern "C" fn hc_test_wrapper(args: *const u128, results: *mut u128) -> *mut c_void {
    info!("hc_test_wrapper called");
    std::ptr::null_mut()
}

pub extern "C" fn hc_init_wrapper(args: *const u128, results: *mut u128) -> *mut c_void {
    info!("hc_init_wrapper called");
    std::ptr::null_mut()
}

pub extern "C" fn hc_shutdown_wrapper(args: *const u128, results: *mut u128) -> *mut c_void {
    info!("hc_shutdown_wrapper called");
    std::ptr::null_mut()
}

pub extern "C" fn hc_get_wrapper(args: *const u128, results: *mut u128) -> *mut c_void {
    info!("hc_get_wrapper called");
    std::ptr::null_mut()
}

pub extern "C" fn hc_put_wrapper(args: *const u128, results: *mut u128) -> *mut c_void {
    info!("hc_put_wrapper called");
    std::ptr::null_mut()
}

pub extern "C" fn hc_call_wrapper(args: *const u128, results: *mut u128) -> *mut c_void {
    info!("hc_call_wrapper called");
    std::ptr::null_mut()
}

pub struct WavmEngine {
    pub engine: Option<*mut c_void>,
    pub sandboxes: HashMap<String, WavmSandbox>,
    pub modules: HashMap<String, WavmModule>,
    pub functions: HashMap<String, WavmFunc>,

    pub ray_hostcall_wrapper_fnmap: HashMap<String, WasmHostCallCPtr>,
    pub ray_hostcall: HashMap<WasmHostCallCPtr, Hostcall>,
}

unsafe impl Sync for WavmEngine {}
unsafe impl Send for WavmEngine {}

impl WavmEngine {
    pub fn new() -> Self {
        let mut map = HashMap::new();
        map.insert("sleep".to_string(), hc_sleep_wrapper as WasmHostCallCPtr);
        map.insert("test".to_string(), hc_test_wrapper as WasmHostCallCPtr);
        map.insert("init".to_string(), hc_init_wrapper as WasmHostCallCPtr);
        map.insert(
            "shutdown".to_string(),
            hc_shutdown_wrapper as WasmHostCallCPtr,
        );
        map.insert("get".to_string(), hc_get_wrapper as WasmHostCallCPtr);
        map.insert("put".to_string(), hc_put_wrapper as WasmHostCallCPtr);
        map.insert("call".to_string(), hc_call_wrapper as WasmHostCallCPtr);
        WavmEngine {
            engine: None,
            sandboxes: HashMap::new(),
            modules: HashMap::new(),
            functions: HashMap::new(),
            ray_hostcall_wrapper_fnmap: map,
            ray_hostcall: HashMap::new(),
        }
    }
}

impl Drop for WavmEngine {
    fn drop(&mut self) {
        if self.engine.is_some() {
            self.functions.clear();
            self.modules.clear();
            self.sandboxes.clear();

            unsafe {
                wasm_engine_delete(self.engine.unwrap());
            }
        }
    }
}

impl WasmEngine for WavmEngine {
    fn init(&mut self) -> Result<()> {
        self.engine = Some(unsafe { wasm_engine_new() });
        Ok(())
    }

    fn compile(&mut self, name: &str, wasm_bytes: &[u8]) -> Result<Box<&dyn WasmModule>> {
        if self.engine.is_none() {
            return Err(anyhow!("Engine is not initialized"));
        }
        let module = unsafe {
            wasm_module_new(
                self.engine.unwrap(),
                wasm_bytes.as_ptr(),
                wasm_bytes.len() as libc::size_t,
            )
        };
        if module.is_null() {
            return Err(anyhow!("Failed to create module"));
        }
        let module = WavmModule {
            name: name.to_string(),
            module,
        };
        self.modules.insert(name.to_string(), module);
        Ok(Box::new(&self.modules[name]))
    }

    fn create_sandbox(&mut self, name: &str) -> Result<Box<&dyn WasmSandbox>> {
        if self.engine.is_none() {
            return Err(anyhow!("Engine is not initialized"));
        }
        let compartment = unsafe { wasm_compartment_new(self.engine.unwrap(), name.as_ptr()) };
        if compartment.is_null() {
            return Err(anyhow!("Failed to create compartment"));
        }
        let store = unsafe { wasm_store_new(compartment, format!("{}_store", name).as_ptr()) };
        if store.is_null() {
            unsafe {
                wasm_compartment_delete(compartment);
            }
            return Err(anyhow!("Failed to create store"));
        }
        let sandbox = WavmSandbox {
            name: name.to_string(),
            compartment,
            store,
            imports: Vec::new(),
            instances: HashMap::new(),
        };
        match self.sandboxes.insert(name.to_string(), sandbox) {
            Some(_) => {
                info!("Sandbox {} already exists", name);
            }
            None => {}
        }
        Ok(Box::new(&self.sandboxes[name]))
    }

    fn instantiate(
        &mut self,
        sandbox_name: &str,
        module_name: &str,
        instance_name: &str,
    ) -> Result<Box<&dyn WasmInstance>> {
        if self.sandboxes.get(sandbox_name).is_none() {
            return Err(anyhow!("Sandbox {} does not exist", sandbox_name));
        }
        match self.sandboxes.get_mut(sandbox_name) {
            Some(sandbox) => {
                if self.modules.get(module_name).is_none() {
                    return Err(anyhow!("Module {} does not exist", module_name));
                }
                let module = self.modules[module_name].module;

                let mut trap_ptr: *mut c_void = std::ptr::null_mut();

                // convert instance name to c string
                let instance_name_cstr = std::ffi::CString::new(instance_name).unwrap();
                let instance = unsafe {
                    // print the imports
                    for import in sandbox.imports.iter() {
                        info!("import: {:x?}", import);
                    }
                    info!(
                        "arguments for wasm_instance_new: {:x?} {:x?} {:x?} {:x?} {:x?}",
                        sandbox.store,
                        module,
                        sandbox.imports.as_mut_ptr(),
                        &mut trap_ptr,
                        instance_name_cstr.as_ptr() as *const u8,
                    );
                    wasm_instance_new(
                        sandbox.store,
                        module,
                        0 as *const *mut c_void, //imports.as_ptr(),
                        0 as *mut *mut c_void,   //&mut trap_ptr,
                        instance_name_cstr.as_ptr() as *const u8,
                    )
                };
                if instance.is_null() {
                    return Err(anyhow!("Failed to create instance"));
                }
                let instance = WavmInstance {
                    name: instance_name.to_string(),
                    instance,
                };
                sandbox
                    .instances
                    .insert(instance_name.to_string(), instance);
            }
            None => {
                return Err(anyhow!("Sandbox {} does not exist", sandbox_name));
            }
        }
        Ok(Box::new(
            &self.sandboxes[sandbox_name].instances[instance_name],
        ))
    }

    fn execute(
        &mut self,
        _sandbox_name: &str,
        _instance_name: &str,
        _func_name: &str,
        _args: Vec<WasmValue>,
    ) -> Result<Vec<WasmValue>> {
        unimplemented!()
    }

    fn has_module(&self, name: &str) -> Result<bool> {
        Ok(self.modules.contains_key(name))
    }

    fn has_sandbox(&self, name: &str) -> Result<bool> {
        Ok(self.sandboxes.contains_key(name))
    }

    fn has_instance(&self, sandbox_name: &str, instance_name: &str) -> Result<bool> {
        unimplemented!()
    }

    fn list_modules(&self) -> Result<Vec<Box<&dyn WasmModule>>> {
        // return a lis of Box<&dyn WasmModule>
        let mut modules: Vec<Box<&dyn WasmModule>> = Vec::new();
        for (_, module) in self.modules.iter() {
            modules.push(Box::new(module));
        }
        Ok(modules)
    }

    fn list_sandboxes(&self) -> Result<Vec<Box<&dyn WasmSandbox>>> {
        // return a lis of Box<&dyn WasmSandbox>
        let mut sandboxes: Vec<Box<&dyn WasmSandbox>> = Vec::new();
        for (_, sandbox) in self.sandboxes.iter() {
            sandboxes.push(Box::new(sandbox));
        }
        Ok(sandboxes)
    }

    fn list_instances(&self, sandbox_name: &str) -> Result<Vec<Box<&dyn WasmInstance>>> {
        unimplemented!()
    }

    fn register_hostcalls(&mut self, sandbox_name: &str, hostcalls: &Hostcalls) -> Result<()> {
        if self.sandboxes.get(sandbox_name).is_none() {
            RayLog::warn(&format!("Sandbox {} does not exist", sandbox_name));
            return Err(anyhow!("Sandbox {} does not exist", sandbox_name));
        }

        let mut imports = Vec::new();

        for hostcall in hostcalls.functions.iter() {
            info!("Registering hostcall {}", hostcall.name);
            unsafe {
                let params = Box::new(hostcall
                    .params
                    .iter()
                    .map(|p| match p {
                        WasmType::I32 => wasm_valtype_new_i32(),
                        WasmType::I64 => wasm_valtype_new_i64(),
                        WasmType::F32 => wasm_valtype_new_f32(),
                        WasmType::F64 => wasm_valtype_new_f64(),
                        WasmType::V128 => wasm_valtype_new_v128(),
                        WasmType::FuncRef => wasm_valtype_new_funcref(),
                        WasmType::ExternRef => wasm_valtype_new_externref(),
                    })
                    .collect::<Vec<_>>());
                let results = Box::new(hostcall
                    .results
                    .iter()
                    .map(|p| match p {
                        WasmType::I32 => wasm_valtype_new_i32(),
                        WasmType::I64 => wasm_valtype_new_i64(),
                        WasmType::F32 => wasm_valtype_new_f32(),
                        WasmType::F64 => wasm_valtype_new_f64(),
                        WasmType::V128 => wasm_valtype_new_v128(),
                        WasmType::FuncRef => wasm_valtype_new_funcref(),
                        WasmType::ExternRef => wasm_valtype_new_externref(),
                    })
                    .collect::<Vec<_>>());
                let params_len = params.len();
                let params_raw = Box::into_raw(params);
                let results_len = results.len();
                let results_raw = Box::into_raw(results);
                info!("Created params and results, param ptr: {:x?} result ptr {:x?}", params_raw, results_raw);
                let ft = wasm_functype_new(
                    params_raw as *const _,
                    params_len as libc::size_t,
                    results_raw as *mut _,
                    results_len as libc::size_t,
                );
                if ft.is_null() {
                    RayLog::warn(&format!("Failed to create function type"));
                    return Err(anyhow!("Failed to create function type"));
                }
                match self.ray_hostcall_wrapper_fnmap.get(&hostcall.name) {
                    Some(wrapper) => {
                        info!("wasm_func_new parameters {:x?} {:x?} {:x?} {:x?}", self.sandboxes[sandbox_name].compartment, ft, *wrapper, std::ffi::CString::new(hostcall.name.as_str()).unwrap().as_ptr() as *const u8);
                        let func = wasm_func_new(
                            self.sandboxes[sandbox_name].compartment,
                            ft,
                            *wrapper,
                            std::ffi::CString::new(hostcall.name.as_str())
                                .unwrap()
                                .as_ptr() as *const u8,
                        );
                        info!("Created function1");

                        imports.push(wasm_func_as_extern(func));

                        let wavm_func = WavmFunc {
                            name: hostcall.name.to_string(),
                            func,
                        };
                        self.functions.insert(hostcall.name.to_string(), wavm_func);
                        self.ray_hostcall.insert(*wrapper, hostcall.clone());
                        WRAPPER_TO_HOSTCALL
                            .write()
                            .unwrap()
                            .insert(*wrapper, hostcall.clone());
                    }
                    None => {
                        warn!("Hostcall {} has no wrapper", hostcall.name);
                    }
                }
                info!("Created function2");
                unsafe {
                    wasm_functype_delete(ft);
                }
            }
        }
        self.sandboxes.get_mut(sandbox_name).unwrap().imports = imports;
        Ok(())
    }

    fn task_loop_once(
        &mut self,
        rt: &Arc<RwLock<Box<dyn RayRuntime + Send + Sync>>>,
    ) -> Result<()> {
        unimplemented!()
    }
}

pub struct WavmSandbox {
    pub name: String,
    pub compartment: *mut c_void,
    pub store: *mut c_void,
    pub imports: Vec<*mut c_void>,

    pub instances: HashMap<String, WavmInstance>,
}

impl WasmSandbox for WavmSandbox {}

impl Drop for WavmSandbox {
    fn drop(&mut self) {
        unsafe {
            wasm_store_delete(self.store);
            wasm_compartment_delete(self.compartment);

            for import in self.imports.iter() {
                wasm_functype_delete(*import);
            }
            for (_, instance) in self.instances.iter() {
                wasm_instance_delete(instance.instance);
            }
        }
    }
}

pub struct WavmModule {
    pub name: String,
    pub module: *mut c_void,
}

impl WasmModule for WavmModule {}

impl Drop for WavmModule {
    fn drop(&mut self) {
        unsafe {
            wasm_module_delete(self.module);
        }
    }
}

pub struct WavmFunc {
    pub name: String,
    pub func: *mut c_void,
}

impl Drop for WavmFunc {
    fn drop(&mut self) {
        unsafe {
            wasm_functype_delete(self.func);
        }
    }
}

pub struct WavmInstance {
    pub name: String,
    pub instance: *mut c_void,
}

impl WasmInstance for WavmInstance {}
