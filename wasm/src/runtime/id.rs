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

const ACTOR_ID_UNIQUE_BYTES: usize = 12;
const TASK_ID_UNIQUE_BYTES: usize = 8;

pub const UNIQUE_ID_SIZE: usize = 28;
pub const JOB_ID_SIZE: usize = 4;

#[allow(dead_code)]
const MAX_OBJECT_ID_INDEX: u32 = ((1 as u64) << 32 - 1) as u32;
pub const ACTOR_ID_SIZE: usize = JOB_ID_SIZE + ACTOR_ID_UNIQUE_BYTES;
pub const TASK_ID_SIZE: usize = ACTOR_ID_SIZE + TASK_ID_UNIQUE_BYTES;
pub const OBJECT_ID_SIZE: usize = TASK_ID_SIZE + 4;

pub const PLACEMENT_GROUP_ID_SIZE: usize = JOB_ID_SIZE + 14;

pub type FunctionID = UniqueID;
pub type ActorClassID = UniqueID;
pub type WorkerID = UniqueID;
pub type ConfigID = UniqueID;
pub type NodeID = UniqueID;

pub trait Base<T> {
    fn size(&self) -> usize;
    fn from_random() -> T;
    fn from_binary(data: &[u8]) -> T;
    fn from_hex_string(hex: &str) -> T;

    fn hex_string(&self) -> String;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ObjectID {
    pub id: [u8; OBJECT_ID_SIZE],
}

impl ObjectID {
    pub fn new() -> ObjectID {
        ObjectID {
            id: [0; OBJECT_ID_SIZE],
        }
    }

    pub fn nil() -> ObjectID {
        static NIL_OBJECT_ID: ObjectID = ObjectID {
            id: [0; OBJECT_ID_SIZE],
        };
        NIL_OBJECT_ID
    }
}

impl Base<ObjectID> for ObjectID {
    fn size(&self) -> usize {
        OBJECT_ID_SIZE
    }

    fn from_random() -> ObjectID {
        unimplemented!()
    }

    fn from_binary(data: &[u8]) -> ObjectID {
        if data.len() != OBJECT_ID_SIZE {
            panic!(
                "ObjectID::from_binary: data length {} != {}",
                data.len(),
                OBJECT_ID_SIZE
            );
        }
        let mut obj = ObjectID::new();
        obj.id.copy_from_slice(data);
        obj
    }

    fn from_hex_string(hex: &str) -> ObjectID {
        // parse hex string
        let mut data = Vec::new();
        for i in 0..OBJECT_ID_SIZE {
            let start = i * 2;
            let end = start + 2;
            let byte = u8::from_str_radix(&hex[start..end], 16).unwrap();
            data.push(byte);
        }
        ObjectID::from_binary(&data)
    }

    fn hex_string(&self) -> String {
        // convert to hex string
        let mut hex = String::new();
        for i in 0..OBJECT_ID_SIZE {
            hex.push_str(&format!("{:02x}", self.id[i]));
        }
        hex
    }
}

#[derive(Clone, Copy, Debug)]
pub struct UniqueID {
    pub id: [u8; UNIQUE_ID_SIZE],
}

impl UniqueID {
    pub fn new() -> UniqueID {
        UniqueID {
            id: [0; UNIQUE_ID_SIZE],
        }
    }

    pub fn nil() -> UniqueID {
        static NIL_UNIQUE_ID: UniqueID = UniqueID {
            id: [0; UNIQUE_ID_SIZE],
        };
        NIL_UNIQUE_ID
    }
}

impl Base<UniqueID> for UniqueID {
    fn size(&self) -> usize {
        UNIQUE_ID_SIZE
    }

    fn from_random() -> UniqueID {
        unimplemented!()
    }

    fn from_binary(data: &[u8]) -> UniqueID {
        if data.len() != UNIQUE_ID_SIZE {
            panic!(
                "UniqueID::from_binary: data length {} != {}",
                data.len(),
                UNIQUE_ID_SIZE
            );
        }
        let mut obj = UniqueID::new();
        obj.id.copy_from_slice(data);
        obj
    }

    fn from_hex_string(hex: &str) -> UniqueID {
        // parse hex string
        let mut data = Vec::new();
        for i in 0..UNIQUE_ID_SIZE {
            let start = i * 2;
            let end = start + 2;
            let byte = u8::from_str_radix(&hex[start..end], 16).unwrap();
            data.push(byte);
        }
        UniqueID::from_binary(&data)
    }

    fn hex_string(&self) -> String {
        // convert to hex string
        let mut hex = String::new();
        for i in 0..UNIQUE_ID_SIZE {
            hex.push_str(&format!("{:02x}", self.id[i]));
        }
        hex
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ActorID {
    pub id: [u8; ACTOR_ID_SIZE],
}

impl ActorID {
    pub fn new() -> ActorID {
        ActorID {
            id: [0; ACTOR_ID_SIZE],
        }
    }

    pub fn nil() -> ActorID {
        static NIL_ACTOR_ID: ActorID = ActorID {
            id: [0; ACTOR_ID_SIZE],
        };
        NIL_ACTOR_ID
    }
}

impl Base<ActorID> for ActorID {
    fn size(&self) -> usize {
        ACTOR_ID_SIZE
    }

    fn from_random() -> ActorID {
        unimplemented!()
    }

    fn from_binary(data: &[u8]) -> ActorID {
        if data.len() != ACTOR_ID_SIZE {
            panic!(
                "ActorID::from_binary: data length {} != {}",
                data.len(),
                ACTOR_ID_SIZE
            );
        }
        let mut obj = ActorID::new();
        obj.id.copy_from_slice(data);
        obj
    }

    fn from_hex_string(hex: &str) -> ActorID {
        // parse hex string
        let mut data = Vec::new();
        for i in 0..ACTOR_ID_SIZE {
            let start = i * 2;
            let end = start + 2;
            let byte = u8::from_str_radix(&hex[start..end], 16).unwrap();
            data.push(byte);
        }
        ActorID::from_binary(&data)
    }

    fn hex_string(&self) -> String {
        // convert to hex string
        let mut hex = String::new();
        for i in 0..ACTOR_ID_SIZE {
            hex.push_str(&format!("{:02x}", self.id[i]));
        }
        hex
    }
}
