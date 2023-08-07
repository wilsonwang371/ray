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

mod wasmedge_engine;
mod wasmtime_engine;

use std::sync::Arc;
use std::sync::{Mutex, RwLock};

use crate::engine::wasmtime_engine::WasmtimeEngine;
use crate::runtime::{ObjectID, RemoteFunctionHolder, WasmTaskExecutionInfo};
use crate::{engine::wasmedge_engine::WasmEdgeEngine, runtime::RayRuntime};

use anyhow::{anyhow, Result};
use core::result::Result::Ok;
use lazy_static::lazy_static;
use rmp::decode::{read_bin_len, read_f32, read_f64, read_i32, read_i64, read_marker};
use rmp::encode::{write_bin, write_f32, write_f64, write_i32, write_i64, write_nil};
use rmp::Marker;

use crate::util::RayLog;
use std::sync::mpsc::{channel, Receiver, Sender};

// a channel for sending task to wasm engine
pub type TaskSender = Sender<WasmTaskExecutionInfo>;
pub type TaskReceiver = Receiver<WasmTaskExecutionInfo>;

// a channel for receiving task result from wasm engine
pub type TaskResultSender = Sender<Result<Vec<u8>>>;
pub type TaskResultReceiver = Receiver<Result<Vec<u8>>>;

// static channel variable for sending task to wasm engine
lazy_static! {
    pub static ref TASK_SENDER: Mutex<Option<TaskSender>> = Mutex::new(None);
    pub static ref TASK_RECEIVER: Mutex<Option<TaskReceiver>> = Mutex::new(None);
    pub static ref TASK_RESULT_SENDER: Mutex<Option<TaskResultSender>> = Mutex::new(None);
    pub static ref TASK_RESULT_RECEIVER: Mutex<Option<TaskResultReceiver>> = Mutex::new(None);
}

pub trait WasmEngine {
    fn init(&self) -> Result<()>;

    fn compile(&mut self, name: &str, wasm_bytes: &[u8]) -> Result<Box<&dyn WasmModule>>;
    fn create_sandbox(&mut self, name: &str) -> Result<Box<&dyn WasmSandbox>>;
    fn instantiate(
        &mut self,
        sandbox_name: &str,
        module_name: &str,
        instance_name: &str,
    ) -> Result<Box<&dyn WasmInstance>>;
    fn execute(
        &mut self,
        sandbox_name: &str,
        instance_name: &str,
        func_name: &str,
        args: Vec<WasmValue>,
    ) -> Result<Vec<WasmValue>>;

    fn has_module(&self, name: &str) -> Result<bool>;
    fn has_sandbox(&self, name: &str) -> Result<bool>;
    fn has_instance(&self, sandbox_name: &str, instance_name: &str) -> Result<bool>;

    fn list_modules(&self) -> Result<Vec<Box<&dyn WasmModule>>>;
    fn list_sandboxes(&self) -> Result<Vec<Box<&dyn WasmSandbox>>>;
    fn list_instances(&self, sandbox_name: &str) -> Result<Vec<Box<&dyn WasmInstance>>>;

    fn register_hostcalls(&mut self, hostcalls: &Hostcalls) -> Result<()>;

    fn task_loop_once(&mut self, rt: &Arc<RwLock<Box<dyn RayRuntime + Send + Sync>>>)
        -> Result<()>;
}

pub trait WasmModule {}

pub trait WasmSandbox {
    // convert to original type
}

pub trait WasmInstance {}

pub enum WasmEngineType {
    WASMEDGE,
    WASMTIME,
    WAMR,
    WAVM,
}

// factory pattern for wasm engine
pub struct WasmEngineFactory {}

impl WasmEngineFactory {
    pub fn create_engine(
        engine_type: WasmEngineType,
        cmdline: Option<&str>,
        dirs: Vec<&str>,
    ) -> Result<Box<dyn WasmEngine + Send + Sync>> {
        let engine: Box<dyn WasmEngine + Send + Sync> = match engine_type {
            WasmEngineType::WASMTIME => Box::new(WasmtimeEngine::new(cmdline, dirs)),
            WasmEngineType::WASMEDGE => Box::new(WasmEdgeEngine::new(cmdline, dirs)),
            _ => {
                return Err(anyhow!("unsupported wasm engine type"));
            }
        };

        // init channels
        let (tx, rx): (TaskSender, TaskReceiver) = channel();
        TASK_SENDER.lock().unwrap().replace(tx);
        TASK_RECEIVER.lock().unwrap().replace(rx);

        let (tx, rx): (TaskResultSender, TaskResultReceiver) = channel();
        TASK_RESULT_SENDER.lock().unwrap().replace(tx);
        TASK_RESULT_RECEIVER.lock().unwrap().replace(rx);

        Ok(engine)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmValue {
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
    V128(u128),
    FuncRef(usize),
    ExternRef(usize),
    Buffer(Box<[u8]>),
}

impl WasmValue {
    // write nil to buffer
    pub fn msgpack_nil_vec() -> Vec<u8> {
        let mut buf = Vec::new();
        write_nil(&mut buf).unwrap();
        buf
    }

    pub fn as_msgpack_vec(&self) -> Result<Vec<u8>> {
        match self {
            WasmValue::I32(v) => {
                let mut buf = Vec::new();
                write_i32(&mut buf, *v).unwrap();
                Ok(buf)
            }
            WasmValue::I64(v) => {
                let mut buf = Vec::new();
                write_i64(&mut buf, *v).unwrap();
                Ok(buf)
            }
            WasmValue::F32(v) => {
                let mut buf = Vec::new();
                let v = f32::from_bits(*v);
                write_f32(&mut buf, v).unwrap();
                Ok(buf)
            }
            WasmValue::F64(v) => {
                let mut buf = Vec::new();
                let v = f64::from_bits(*v);
                write_f64(&mut buf, v).unwrap();
                Ok(buf)
            }
            WasmValue::Buffer(v) => {
                let mut buf = Vec::new();
                write_bin(&mut buf, v.as_ref()).unwrap();
                Ok(buf)
            }
            _ => {
                return Err(anyhow!("unsupported argument type"));
            }
        }
    }

    pub fn from_msgpack_vec_array(buf_list: Vec<&[u8]>) -> Result<Vec<WasmValue>> {
        let mut args: Vec<WasmValue> = vec![];
        for arg_buf in buf_list {
            // read message pack data from buffer
            let marker_data = Vec::from(&arg_buf[0..1]);
            match read_marker(&mut marker_data.as_slice()) {
                Ok(m) => match m {
                    Marker::I32 => match WasmValue::from_msgpack_vec(WasmType::I32, &arg_buf) {
                        Ok(wasm_value) => {
                            args.push(wasm_value);
                            continue;
                        }
                        Err(e) => {
                            RayLog::error(&format!("convert msgpack to WasmValue error: {}", e));
                            return Err(anyhow!("convert msgpack to WasmValue error"));
                        }
                    },
                    Marker::I64 => match WasmValue::from_msgpack_vec(WasmType::I64, &arg_buf) {
                        Ok(wasm_value) => {
                            args.push(wasm_value);
                            continue;
                        }
                        Err(e) => {
                            RayLog::error(&format!("convert msgpack to WasmValue error: {}", e));
                            return Err(anyhow!("convert msgpack to WasmValue error"));
                        }
                    },
                    Marker::F32 => match WasmValue::from_msgpack_vec(WasmType::F32, &arg_buf) {
                        Ok(wasm_value) => {
                            args.push(wasm_value);
                            continue;
                        }
                        Err(e) => {
                            RayLog::error(&format!("convert msgpack to WasmValue error: {}", e));
                            return Err(anyhow!("convert msgpack to WasmValue error"));
                        }
                    },
                    Marker::F64 => match WasmValue::from_msgpack_vec(WasmType::F64, &arg_buf) {
                        Ok(wasm_value) => {
                            args.push(wasm_value);
                            continue;
                        }
                        Err(e) => {
                            RayLog::error(&format!("convert msgpack to WasmValue error: {}", e));
                            return Err(anyhow!("convert msgpack to WasmValue error"));
                        }
                    },
                    Marker::Bin16 | Marker::Bin32 | Marker::Bin8 => {
                        match WasmValue::from_msgpack_vec(WasmType::Buffer, &arg_buf) {
                            Ok(wasm_value) => {
                                // RayLog::info(&format!("decode incoming buffer: {:?}", wasm_value));
                                args.push(wasm_value);
                                continue;
                            }
                            Err(e) => {
                                RayLog::error(&format!(
                                    "convert msgpack to WasmValue error: {}",
                                    e
                                ));
                                return Err(anyhow!("convert msgpack to WasmValue error"));
                            }
                        }
                    }
                    _ => {
                        RayLog::error(&format!("unsupported marker: {:?}", m));
                        return Err(anyhow!("unsupported marker"));
                    }
                },
                Err(e) => {
                    RayLog::error(&format!("read marker error: {:?}", e));
                    return Err(anyhow!("read marker error"));
                }
            }
        }
        Ok(args)
    }

    pub fn from_msgpack_vec(ty: WasmType, data: &[u8]) -> Result<WasmValue> {
        match ty {
            WasmType::I32 => {
                let mut buf = data;
                let val = read_i32(&mut buf).unwrap();
                Ok(WasmValue::I32(val))
            }
            WasmType::I64 => {
                let mut buf = data;
                let val = read_i64(&mut buf).unwrap();
                Ok(WasmValue::I64(val))
            }
            WasmType::F32 => {
                let mut buf = data;
                let val = read_f32(&mut buf).unwrap();
                let val = f32::to_ne_bytes(val);
                // convert [u8; 4] to u32
                let val = u32::from_ne_bytes(val.try_into().unwrap());
                Ok(WasmValue::F32(val))
            }
            WasmType::F64 => {
                let mut buf = data;
                let val = read_f64(&mut buf).unwrap();
                let val = f64::to_ne_bytes(val);
                // convert [u8; 8] to u64
                let val = u64::from_ne_bytes(val.try_into().unwrap());
                Ok(WasmValue::F64(val))
            }
            WasmType::Buffer => {
                let mut buf = data;
                let len = read_bin_len(&mut buf).unwrap();
                // read len bytes
                let val = buf[..len as usize].to_vec();
                Ok(WasmValue::Buffer(val.into_boxed_slice()))
            }
            _ => {
                return Err(anyhow!("unsupported argument type"));
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmType {
    I32,
    I64,
    F32,
    F64,
    V128,
    FuncRef,
    ExternRef,
    Buffer,
}

#[derive(Clone)]
pub struct Hostcalls {
    pub module_name: String,
    pub functions: Vec<Hostcall>,
    pub runtime: Arc<RwLock<Box<dyn RayRuntime + Send + Sync>>>,
}

#[derive(Clone)]
pub struct Hostcall {
    pub name: String,
    pub params: Vec<WasmType>,
    pub results: Vec<WasmType>,
    pub func: fn(&mut dyn WasmContext, &[WasmValue]) -> Result<Vec<WasmValue>>,
}

#[derive(Debug, Clone)]
pub struct WasmFunc {
    pub sandbox: String,
    pub module: String,
    pub name: String,
    pub params: Vec<WasmType>,
    pub results: Vec<WasmType>,
}

impl WasmFunc {
    pub fn new(
        sandbox: String,
        module: String,
        name: String,
        params: Vec<WasmType>,
        results: Vec<WasmType>,
    ) -> Self {
        WasmFunc {
            sandbox,
            name,
            module,
            params,
            results,
        }
    }

    /// Get the number of parameters
    pub fn params_count(&self) -> usize {
        self.params.len()
    }

    /// Get the size of the data at the given index
    pub fn param_data_size(&self, idx: usize, promoted: bool) -> Result<usize> {
        if idx >= self.params.len() {
            return Err(anyhow!("invalid argument index"));
        }
        match self.params.get(idx).unwrap() {
            WasmType::I32 => Ok(4),
            WasmType::I64 => Ok(8),
            WasmType::F32 => {
                if !promoted {
                    Ok(4)
                } else {
                    Ok(8)
                }
            }
            WasmType::F64 => Ok(8),
            _ => Err(anyhow!("unsupported argument type")),
        }
    }

    /// Get the size of the data needed by all the parameters
    pub fn param_data_total_size(&self) -> Result<usize> {
        let mut total_size = 0;
        for i in self.params.iter() {
            match i {
                WasmType::I32 => {
                    total_size += 4;
                }
                WasmType::I64 => {
                    total_size += 8;
                }
                WasmType::F32 => {
                    total_size += 4;
                }
                WasmType::F64 => {
                    total_size += 8;
                }
                _ => {
                    return Err(anyhow!("unsupported argument type"));
                }
            }
        }
        Ok(total_size)
    }

    /// Create a vector of all the parsed parameters from raw memory data
    pub fn params_convert(&self, data: &[u8], promoted: bool) -> Result<Vec<WasmValue>> {
        if data.len() < self.param_data_total_size()? {
            return Err(anyhow!("invalid data size"));
        }
        // iterate all arguments and put them into a arguments vector
        let mut args = vec![];
        let mut param_offset = 0;
        for i in self.params.iter() {
            match i {
                WasmType::I32 => {
                    // convert 4 bytes to i32
                    let val = i32::from_ne_bytes(
                        data[param_offset..param_offset + 4].try_into().unwrap(),
                    );
                    let arg = WasmValue::I32(val);
                    args.push(arg);
                    param_offset += 4;
                }
                WasmType::I64 => {
                    // convert 8 bytes to i64
                    let val = i64::from_ne_bytes(
                        data[param_offset..param_offset + 8].try_into().unwrap(),
                    );
                    let arg = WasmValue::I64(val);
                    args.push(arg);
                    param_offset += 8;
                }
                WasmType::F32 => {
                    let val;
                    if promoted {
                        // convert 8 bytes to f64
                        let tmp_val = f64::from_ne_bytes(
                            data[param_offset..param_offset + 8].try_into().unwrap(),
                        );
                        // convert f64 to f32
                        let tmp_val = tmp_val as f32;
                        let tmp_data = tmp_val.to_ne_bytes();
                        val = u32::from_ne_bytes(tmp_data.try_into().unwrap());
                        param_offset += 8;
                    } else {
                        let tmp_data = &data[param_offset..param_offset + 4];
                        val = u32::from_ne_bytes(tmp_data.try_into().unwrap());
                        param_offset += 4;
                    }
                    // convert 4 bytes to u32
                    let arg = WasmValue::F32(val);
                    args.push(arg);
                }
                WasmType::F64 => {
                    // convert 8 bytes to u64
                    let val = u64::from_ne_bytes(
                        data[param_offset..param_offset + 8].try_into().unwrap(),
                    );
                    let arg = WasmValue::F64(val);
                    args.push(arg);
                    param_offset += 8;
                }
                _ => {
                    return Err(anyhow!("unsupported argument type"));
                }
            }
        }
        Ok(args)
    }
}

impl Hostcalls {
    pub fn new(module_name: &str, runtime: Arc<RwLock<Box<dyn RayRuntime + Send + Sync>>>) -> Self {
        Hostcalls {
            module_name: module_name.to_string(),
            functions: Vec::new(),
            runtime,
        }
    }

    pub fn add_hostcall(
        &mut self,
        name: &str,
        params: Vec<WasmType>,
        results: Vec<WasmType>,
        func: fn(&mut dyn WasmContext, &[WasmValue]) -> Result<Vec<WasmValue>>,
    ) -> Result<()> {
        if self.functions.iter().any(|f| f.name == name) {
            return Err(anyhow!("Hostcall {} already exists", name));
        }
        self.functions.push(Hostcall {
            name: name.to_string(),
            params,
            results,
            func,
        });
        Ok(())
    }

    pub fn remove_hostcall(&mut self, name: &str) -> Result<()> {
        if self.functions.iter().any(|f| f.name == name) {
            return Err(anyhow!("Hostcall {} does not exist", name));
        }
        self.functions.retain(|f| f.name != name);
        Ok(())
    }
}

pub trait WasmContext {
    fn get_memory_region(&mut self, off: usize, len: usize) -> Result<&[u8]>;
    fn get_memory_region_mut(&mut self, off: usize, len: usize) -> Result<&mut [u8]>;
    fn get_func_ref(&mut self, func_idx: u32) -> Result<WasmFunc>;
    fn invoke(
        &mut self,
        remote_func: &RemoteFunctionHolder,
        args: &[WasmValue],
    ) -> Result<Vec<ObjectID>>;
    fn get_object(&mut self, object_id: &ObjectID) -> Result<Vec<u8>>;
    fn put_object(&mut self, data: &[u8]) -> Result<ObjectID>;
    fn submit_sandbox_binary(&mut self) -> Result<()>;

    // functions to track mem allocation & free
    fn track_mem_ops(&mut self, mem_ptr: u32, mem_size: usize, is_free: bool) -> Result<()>;
    fn lookup_mem_alloc(&mut self, mem_ptr: u32) -> Result<usize>;
}
