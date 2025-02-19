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
use clap::{Parser, ValueEnum};
use std::net::UdpSocket;

use crate::runtime::core_worker::{
    RayLog_Debug, RayLog_Error, RayLog_Fatal, RayLog_Info, RayLog_Warn,
};

use anyhow::{anyhow, Result};
use core::result::Result::Ok;

use rmp::decode::{read_f32, read_f64, read_i32, read_i64, read_marker, read_u32, read_u64};
use rmp::encode::write_bin;

use rmp::Marker;

pub struct RayLog;

impl RayLog {
    pub fn info(msg: &str) {
        unsafe {
            RayLog_Info(msg.as_ptr(), msg.len());
        }
    }

    pub fn warn(msg: &str) {
        unsafe {
            RayLog_Warn(msg.as_ptr(), msg.len());
        }
    }

    pub fn error(msg: &str) {
        unsafe {
            RayLog_Error(msg.as_ptr(), msg.len());
        }
    }

    pub fn fatal(msg: &str) {
        unsafe {
            RayLog_Fatal(msg.as_ptr(), msg.len());
        }
    }

    pub fn debug(msg: &str) {
        unsafe {
            RayLog_Debug(msg.as_ptr(), msg.len());
        }
    }
}

pub fn get_node_ip_address(address: &str) -> String {
    // convert str to String
    let mut ip_address = address.to_string();
    if ip_address.is_empty() {
        ip_address = "8.8.8.8:53".to_string();
    }
    // use udp resolver to get local ip address
    let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind socket");
    socket
        .connect(ip_address)
        .expect("Failed to connect to DNS server");
    let local_addr = socket.local_addr().expect("Failed to get local address");
    local_addr.ip().to_string()
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct WorkerParameters {
    /// The address of the Ray cluster to connect to.
    #[arg(long, verbatim_doc_comment)]
    pub ray_address: Option<String>,

    /// Prevents external clients without the password from connecting to Redis
    /// if provided.
    #[arg(long, verbatim_doc_comment)]
    pub ray_redis_password: Option<String>,

    /// A list of directories or files of dynamic libraries that specify the
    /// search path for user code. Only searching the top level under a directory.
    /// ':' is used as the separator.
    #[arg(long, verbatim_doc_comment)]
    pub ray_code_search_path: Option<String>,

    /// Assigned job id
    #[arg(long, verbatim_doc_comment)]
    pub ray_job_id: Option<String>,

    /// The port to use for the node manager
    #[arg(long, verbatim_doc_comment)]
    pub ray_node_manager_port: Option<i32>,

    /// It will specify the socket name used by the raylet if provided.
    #[arg(long, verbatim_doc_comment)]
    pub ray_raylet_socket_name: Option<String>,

    /// It will specify the socket name used by the plasma store if provided.
    #[arg(long, verbatim_doc_comment)]
    pub ray_plasma_store_socket_name: Option<String>,

    /// The path of this session.
    #[arg(long, verbatim_doc_comment)]
    pub ray_session_dir: Option<String>,

    /// Logs dir for workers.
    #[arg(long, verbatim_doc_comment)]
    pub ray_logs_dir: Option<String>,

    /// The ip address for this node.
    #[arg(long, verbatim_doc_comment)]
    pub ray_node_ip_address: Option<String>,

    /// The command line args to be appended as parameters of the `ray start`
    /// command. It takes effect only if Ray head is started by a driver. Run `ray
    /// start --help` for details.
    #[arg(long, verbatim_doc_comment)]
    pub ray_head_args: Option<String>,

    /// The startup token assigned to this worker process by the raylet.
    #[arg(long, verbatim_doc_comment)]
    pub startup_token: Option<i64>,

    /// The default actor lifetime type, `detached` or `non_detached`.
    #[arg(long, verbatim_doc_comment)]
    pub ray_default_actor_lifetime: Option<String>,

    /// The serialized runtime env.
    #[arg(long, verbatim_doc_comment)]
    pub ray_runtime_env: Option<String>,

    /// The computed hash of the runtime env for this worker.
    #[arg(long, verbatim_doc_comment)]
    pub ray_runtime_env_hash: Option<i32>,

    /// The namespace of job. If not set,
    /// a unique value will be randomly generated.
    #[arg(long, verbatim_doc_comment)]
    pub ray_job_namespace: Option<String>,

    /// type of the wasm engine to use
    #[arg(long, value_enum, verbatim_doc_comment, default_value_t = WasmEngineTypeParam::WASMEDGE)]
    pub engine_type: WasmEngineTypeParam,
}

impl WorkerParameters {
    pub fn new_empty() -> Self {
        Self {
            ray_address: None,
            ray_redis_password: None,
            ray_code_search_path: None,
            ray_job_id: None,
            ray_node_manager_port: None,
            ray_raylet_socket_name: None,
            ray_plasma_store_socket_name: None,
            ray_session_dir: None,
            ray_logs_dir: None,
            ray_node_ip_address: None,
            ray_head_args: None,
            startup_token: None,
            ray_default_actor_lifetime: None,
            ray_runtime_env: None,
            ray_runtime_env_hash: None,
            ray_job_namespace: None,
            engine_type: WasmEngineTypeParam::WASMTIME,
        }
    }
}

#[derive(ValueEnum, Debug, Clone)]
pub enum WasmEngineTypeParam {
    WASMEDGE,
    WASMTIME,
    WAVM,
    WAMR,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum WasmFileFormat {
    WASM,
    WAT,
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct LauncherParameters {
    /// the path to the wasm file
    #[arg(short = 'f', long, verbatim_doc_comment)]
    pub file: String,

    /// the type of the input file
    #[arg(short = 't', long, value_enum, verbatim_doc_comment, default_value_t = WasmFileFormat::WASM)]
    pub file_format: WasmFileFormat,

    /// type of the wasm engine to use
    #[arg(short = 'e', long, value_enum, verbatim_doc_comment, default_value_t = WasmEngineTypeParam::WASMEDGE)]
    pub engine_type: WasmEngineTypeParam,

    /// the entry point function name
    /// if not set, the default value is `_start`
    #[arg(short = 's', long, verbatim_doc_comment, default_value = "_start")]
    pub entry_point: String,

    /// command line arguments for the entry point function
    /// if not set, the default value is empty string
    /// the arguments are separated by space
    /// e.g. "arg1 arg2 arg3"
    /// if the argument contains space, it should be quoted
    /// e.g. "arg1 arg2 \"arg 3\""
    #[arg(
        short = 'a',
        long,
        value_name = "ARG1 ARG2 ...",
        verbatim_doc_comment,
        default_value = ""
    )]
    pub args: Option<String>,

    /// binding directories into wasm virtual file system
    /// this argument can be used multiple times
    /// the directories are in the format of "guest_dir:host_dir" or
    /// "host_dir"
    /// e.g. "/tmp:/tmp" or "/tmp"
    #[arg(short = 'd', long = "dir", value_name = "GUEST_DIR:HOST_DIR", verbatim_doc_comment, default_value = None)]
    pub dirs: Option<Vec<String>>,
}

pub enum SerDesType {
    MsgPack,
    JSON,
}

pub trait SerDes {
    fn serialize(&self, data: &[u8]) -> Result<Vec<u8>>;
    fn deserialize(&self, data: &[u8]) -> Result<Vec<u8>>;
}

pub struct SerDesFactory {}

impl SerDesFactory {
    pub fn create(serdes_type: SerDesType) -> Box<dyn SerDes> {
        match serdes_type {
            SerDesType::MsgPack => Box::new(MsgPackSerDes {}),
            SerDesType::JSON => unimplemented!(),
        }
    }
}

struct MsgPackSerDes {}

impl SerDes for MsgPackSerDes {
    fn serialize(&self, data: &[u8]) -> Result<Vec<u8>> {
        self.serialize_msgpack(data)
    }

    fn deserialize(&self, data: &[u8]) -> Result<Vec<u8>> {
        self.deserialize_msgpack(data)
    }
}

impl MsgPackSerDes {
    fn serialize_msgpack(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        match write_bin(&mut buf, data) {
            Ok(_) => Ok(buf),
            Err(e) => Err(anyhow!("serialize_msgpack failed: {:?}", e)),
        }
    }

    fn deserialize_msgpack(&self, data: &[u8]) -> Result<Vec<u8>> {
        let marker_data = Vec::from(&data[0..1]);
        match read_marker(&mut marker_data.as_slice()) {
            Ok(m) => match m {
                Marker::FixPos(_) => unimplemented!(),
                Marker::FixNeg(_) => unimplemented!(),
                Marker::Null => Ok(vec![]),
                Marker::True => unimplemented!(),
                Marker::False => unimplemented!(),
                Marker::U8 => unimplemented!(),
                Marker::U16 => unimplemented!(),
                Marker::U32 => {
                    let mut buf = data;
                    let val = read_u32(&mut buf).unwrap();
                    Ok(val.to_ne_bytes().to_vec())
                }
                Marker::U64 => {
                    let mut buf = data;
                    let val = read_u64(&mut buf).unwrap();
                    Ok(val.to_ne_bytes().to_vec())
                }
                Marker::I8 => unimplemented!(),
                Marker::I16 => unimplemented!(),
                Marker::I32 => {
                    let mut buf = data;
                    let val = read_i32(&mut buf).unwrap();
                    Ok(val.to_ne_bytes().to_vec())
                }
                Marker::I64 => {
                    let mut buf = data;
                    let val = read_i64(&mut buf).unwrap();
                    Ok(val.to_ne_bytes().to_vec())
                }
                Marker::F32 => {
                    let mut buf = data;
                    let val = read_f32(&mut buf).unwrap();
                    Ok(val.to_ne_bytes().to_vec())
                }
                Marker::F64 => {
                    let mut buf = data;
                    let val = read_f64(&mut buf).unwrap();
                    Ok(val.to_ne_bytes().to_vec())
                }
                Marker::FixStr(_) => unimplemented!(),
                Marker::Str8 => unimplemented!(),
                Marker::Str16 => unimplemented!(),
                Marker::Str32 => unimplemented!(),
                Marker::Bin8 => {
                    let buf = data;
                    let count = buf[1] as usize;
                    let mut val = vec![0u8; count as usize];
                    val.clone_from_slice(&buf[2..(count + 2) as usize]);
                    Ok(val)
                }
                Marker::Bin16 => {
                    let buf = data;
                    let count: usize = u16::from_ne_bytes(buf[1..3].try_into().unwrap())
                        .try_into()
                        .unwrap();
                    let mut val = vec![0u8; count as usize];
                    val.clone_from_slice(&buf[3..(count + 3) as usize]);
                    Ok(val)
                }
                Marker::Bin32 => {
                    let buf = data;
                    let count: usize = u32::from_ne_bytes(buf[1..5].try_into().unwrap())
                        .try_into()
                        .unwrap();
                    let mut val = vec![0u8; count as usize];
                    val.clone_from_slice(&buf[5..(count + 5) as usize]);
                    Ok(val)
                }
                Marker::FixArray(_) => unimplemented!(),
                Marker::Array16 => unimplemented!(),
                Marker::Array32 => unimplemented!(),
                Marker::FixMap(_) => unimplemented!(),
                Marker::Map16 => unimplemented!(),
                Marker::Map32 => unimplemented!(),
                Marker::FixExt1 => unimplemented!(),
                Marker::FixExt2 => unimplemented!(),
                Marker::FixExt4 => unimplemented!(),
                Marker::FixExt8 => unimplemented!(),
                Marker::FixExt16 => unimplemented!(),
                Marker::Ext8 => unimplemented!(),
                Marker::Ext16 => unimplemented!(),
                Marker::Ext32 => unimplemented!(),
                Marker::Reserved => unimplemented!(),
            },
            Err(e) => {
                return Err(anyhow!("deserialize_msgpack failed: {:?}", e));
            }
        }
    }
}
