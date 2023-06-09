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

use crate::engine::{WasmType, WasmValue};

use libc::memcpy;
use wasmedge_sys as sys;
use wasmedge_types::ValType;

pub fn wasmedgetype_to_wasmtype(t: &ValType) -> WasmType {
    match t {
        ValType::I32 => WasmType::I32,
        ValType::I64 => WasmType::I64,
        ValType::F32 => WasmType::F32,
        ValType::F64 => WasmType::F64,
        ValType::V128 => WasmType::V128,
        ValType::ExternRef => WasmType::ExternRef,
        ValType::FuncRef => WasmType::FuncRef,
    }
}

pub fn wasmtype_to_wasmedgetype(t: &WasmType) -> ValType {
    match t {
        WasmType::I32 => ValType::I32,
        WasmType::I64 => ValType::I64,
        WasmType::F32 => ValType::F32,
        WasmType::F64 => ValType::F64,
        WasmType::V128 => ValType::V128,
        WasmType::ExternRef => ValType::ExternRef,
        WasmType::FuncRef => ValType::FuncRef,
        WasmType::Buffer => ValType::I32,
    }
}

pub fn from_wasmedge_value(val: &sys::WasmValue) -> WasmValue {
    match val.ty() {
        ValType::I32 => WasmValue::I32(val.to_i32()),
        ValType::I64 => WasmValue::I64(val.to_i64()),
        ValType::F32 => {
            let mut buf = [0u8; 4];
            unsafe {
                memcpy(
                    buf.as_mut_ptr() as *mut libc::c_void,
                    val.to_f32().to_le_bytes().as_ptr() as *const libc::c_void,
                    4,
                );
            }
            WasmValue::F32(unsafe { std::mem::transmute(buf) })
        }
        ValType::F64 => {
            let mut buf = [0u8; 8];
            unsafe {
                memcpy(
                    buf.as_mut_ptr() as *mut libc::c_void,
                    val.to_f64().to_le_bytes().as_ptr() as *const libc::c_void,
                    8,
                );
            }
            WasmValue::F64(unsafe { std::mem::transmute(buf) })
        }
        ValType::V128 => WasmValue::V128(val.to_v128() as u128),
        ValType::FuncRef => unimplemented!("FuncRef"),
        ValType::ExternRef => unimplemented!("ExternRef"),
    }
}

pub fn to_wasmedge_value(val: &WasmValue) -> (sys::WasmValue, Option<Box<[u8]>>) {
    match val {
        WasmValue::I32(v) => (sys::WasmValue::from_i32(*v), None),
        WasmValue::I64(v) => (sys::WasmValue::from_i64(*v), None),
        WasmValue::F32(v) => {
            let mut buf = [0u8; 4];
            buf.copy_from_slice(&v.to_le_bytes());
            (
                sys::WasmValue::from_f32(unsafe { std::mem::transmute(buf) }),
                None,
            )
        }
        WasmValue::F64(v) => {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&v.to_le_bytes());
            (
                sys::WasmValue::from_f64(unsafe { std::mem::transmute(buf) }),
                None,
            )
        }
        WasmValue::V128(v) => (sys::WasmValue::from_v128(*v as i128), None),
        WasmValue::FuncRef(_) => unimplemented!("FuncRef"),
        WasmValue::ExternRef(_) => unimplemented!("ExternRef"),
        WasmValue::Buffer(v) => (
            sys::WasmValue::from_i32(0 as i32),
            Some(v.to_vec().into_boxed_slice()),
        ),
    }
}
