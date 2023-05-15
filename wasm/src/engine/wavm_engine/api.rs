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

use super::WasmHostCallCPtr;
use libc::c_void;

extern "C" {
    pub fn wasm_engine_new() -> *mut c_void;
    pub fn wasm_engine_delete(engine: *mut c_void);

    pub fn wasm_compartment_new(engine: *mut c_void, name: *const u8) -> *mut c_void;
    pub fn wasm_compartment_delete(compartment: *mut c_void);

    pub fn wasm_store_new(compartment: *mut c_void, name: *const u8) -> *mut c_void;
    pub fn wasm_store_delete(store: *mut c_void);

    pub fn wasm_module_new(
        engine: *mut c_void,
        wasm_bytes: *const u8,
        wasm_bytes_len: libc::size_t,
    ) -> *mut c_void;
    pub fn wasm_module_delete(module: *mut c_void);

    pub fn wasm_functype_new(
        params: *const *mut c_void,
        params_len: libc::size_t,
        results: *const *mut c_void,
        results_len: libc::size_t,
    ) -> *mut c_void;
    pub fn wasm_functype_delete(ft: *mut c_void);

    pub fn wasm_func_new(
        compartment: *mut c_void,
        ft: *mut c_void,
        func_ptr: WasmHostCallCPtr,
        name: *const u8,
    ) -> *mut c_void;

    pub fn wasm_func_as_extern(func: *mut c_void) -> *mut c_void;

    pub fn wasm_instance_new(
        store: *mut c_void,
        module: *mut c_void,
        imports: *const *mut c_void,
        trap_ptr: *mut *mut c_void,
        debug_name: *const u8,
    ) -> *mut c_void;
    pub fn wasm_instance_delete(instance: *mut c_void);

    pub fn wasm_valtype_new(kind: i32) -> *mut c_void;
}

const WASM_I32: i32 = 0;
const WASM_I64: i32 = 1;
const WASM_F32: i32 = 2;
const WASM_F64: i32 = 3;
const WASM_V128: i32 = 4;
const WASM_ANYREF: i32 = 128;
const WASM_FUNCREF: i32 = 129;

pub fn wasm_valtype_new_i32() -> *mut c_void {
    unsafe { wasm_valtype_new(WASM_I32) }
}

pub fn wasm_valtype_new_i64() -> *mut c_void {
    unsafe { wasm_valtype_new(WASM_I64) }
}

pub fn wasm_valtype_new_f32() -> *mut c_void {
    unsafe { wasm_valtype_new(WASM_F32) }
}

pub fn wasm_valtype_new_f64() -> *mut c_void {
    unsafe { wasm_valtype_new(WASM_F64) }
}

pub fn wasm_valtype_new_v128() -> *mut c_void {
    unsafe { wasm_valtype_new(WASM_V128) }
}

pub fn wasm_valtype_new_funcref() -> *mut c_void {
    unsafe { wasm_valtype_new(WASM_FUNCREF) }
}

pub fn wasm_valtype_new_externref() -> *mut c_void {
    unsafe { wasm_valtype_new(WASM_ANYREF) }
}
