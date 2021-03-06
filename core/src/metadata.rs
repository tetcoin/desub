// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of substrate-desub.
//
// substrate-desub is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// substrate-desub is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with substrate-desub.  If not, see <http://www.gnu.org/licenses/>.

// taken directly and modified from substrate-subxt:
// https://github.com/paritytech/substrate-subxt

#[cfg(test)]
pub mod test_suite;
mod version_07;
mod version_08;
mod version_09;
mod version_10;
mod version_11;
mod versions;

use codec::{Decode, Encode, EncodeAsRef, HasCompact};
use codec411::Decode as OldDecode;
use failure::Fail;
use runtime_metadata_latest::{
    DecodeDifferent, RuntimeMetadata, RuntimeMetadataPrefixed, StorageEntryModifier,
    StorageEntryType, StorageHasher, META_RESERVED,
};

use std::{
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
    marker::PhantomData,
    rc::Rc,
    str::FromStr,
};
use substrate_primitives::storage::StorageKey;

/// Newtype struct around a Vec<u8> (vector of bytes)
#[derive(Clone)]
pub struct Encoded(pub Vec<u8>);

impl Encode for Encoded {
    fn encode(&self) -> Vec<u8> {
        self.0.to_owned()
    }
}

pub fn compact<T: HasCompact>(t: T) -> Encoded {
    let encodable: <<T as HasCompact>::Type as EncodeAsRef<'_, T>>::RefType =
        From::from(&t);
    Encoded(encodable.encode())
}

#[derive(Debug, Clone, derive_more::Display)]
pub enum MetadataError {
    ModuleNotFound(String),
    CallNotFound(&'static str),
    EventNotFound(u8),
    StorageNotFound(&'static str),
    StorageTypeError,
    MapValueTypeError,
}

#[derive(Clone, Debug, PartialEq)]
/// Metadata struct encompassing calls, storage, and events
pub struct Metadata {
    /// Hashmap of Modules (name -> module-specific metadata)
    modules: HashMap<String, Rc<ModuleMetadata>>,
    modules_by_event_index: HashMap<u8, String>,
}

impl Metadata {
    /// Create a new Metadata type from raw encoded bytes
    ///
    /// # Panics
    /// Panics is the metadata version is not supported,
    /// or the version is invalid
    ///
    /// Panics if decoding into metadata prefixed fails
    pub fn new(bytes: &[u8]) -> Self {
        // Runtime metadata is a tuple struct with the following fields:
        // RuntimeMetadataPrefixed(u32, RuntimeMetadata)
        // this means when it's SCALE encoded, the first four bytes
        // are the 'u32' prefix, and since `RuntimeMetadata` is an enum,
        // the first byte is the index of the enum item.
        // Since RuntimeMetadata is versioned starting from 0, this also corresponds to
        // the Metadata version
        let version = bytes[4];

        match version {
            0x07 => {
                let meta: runtime_metadata07::RuntimeMetadataPrefixed =
                    OldDecode::decode(&mut &bytes[..]).expect("Decode failed");
                meta.try_into().expect("Conversion failed")
            }
            0x08 => {
                let meta: runtime_metadata08::RuntimeMetadataPrefixed =
                    Decode::decode(&mut &bytes[..]).expect("Decode failed");
                meta.try_into().expect("Conversion failed")
            }
            0x09 => {
                let meta: runtime_metadata09::RuntimeMetadataPrefixed =
                    Decode::decode(&mut &bytes[..]).expect("Decode failed");
                meta.try_into().expect("Conversion failed")
            }
            0xA => {
                let meta: runtime_metadata10::RuntimeMetadataPrefixed =
                    Decode::decode(&mut &bytes[..]).expect("Decode failed");
                meta.try_into().expect("Conversion failed")
            },
            0xB => {
                let meta: runtime_metadata_latest::RuntimeMetadataPrefixed =
                    Decode::decode(&mut &bytes[..]).expect("Decode failed");
                meta.try_into().expect("Conversion failed")
            }
            e @ _ => panic!("version {} is unknown, invalid or unsupported", e), /* TODO remove panic */
        }
    }

    /// returns an iterate over all Modules
    pub fn modules(&self) -> impl Iterator<Item = &Rc<ModuleMetadata>> {
        self.modules.values()
    }

    /// returns a weak reference to a module from it's name
    pub fn module<S>(&self, name: S) -> Result<Rc<ModuleMetadata>, MetadataError>
    where
        S: ToString,
    {
        let name = name.to_string();
        self.modules
            .get(&name)
            .ok_or(MetadataError::ModuleNotFound(name))
            .map(|m| (*m).clone())
    }

    pub fn module_exists<S>(&self, name: S) -> bool
    where
        S: ToString,
    {
        let name = name.to_string();
        self.modules.get(&name).is_some()
    }

    /// get the name of a module given it's event index
    pub fn module_name(&self, module_index: u8) -> Result<String, MetadataError> {
        self.modules_by_event_index
            .get(&module_index)
            .cloned()
            .ok_or(MetadataError::EventNotFound(module_index))
    }

    /// print out a detailed but human readable description of the module
    /// metadata
    pub fn detailed_pretty(&self) -> String {
        let mut string = String::new();
        for (name, module) in &self.modules {
            string.push_str(name.as_str());
            string.push('\n');
            for (storage, meta) in &module.storage {
                string.push_str(" S  ");
                string.push_str(storage.as_str());
                string.push_str(format!(" TYPE {:?}", meta.ty).as_str());
                string.push_str(format!(" MOD {:?}", meta.modifier).as_str());
                string.push('\n');
            }
            for (call, _) in &module.calls {
                string.push_str(" C  ");
                string.push_str(call.as_str());
                string.push('\n');
            }
            for (_, event) in &module.events {
                string.push_str(" E  ");
                string.push_str(event.name.as_str());
                string.push('\n');
            }
        }
        string
    }

    /// print out a human readable description of the metadata
    pub fn pretty(&self) -> String {
        let mut string = String::new();
        for (name, module) in &self.modules {
            string.push_str(name.as_str());
            string.push('\n');
            for (storage, _) in &module.storage {
                string.push_str(" s  ");
                string.push_str(storage.as_str());
                string.push('\n');
            }
            for (call, _) in &module.calls {
                string.push_str(" c  ");
                string.push_str(call.as_str());
                string.push('\n');
            }
            for (_, event) in &module.events {
                string.push_str(" e  ");
                string.push_str(event.name.as_str());
                string.push('\n');
            }
        }
        string
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleMetadata {
    /// index of the module within StorageMetadata 'Entries'
    index: u8,
    /// name of the module
    name: String,
    /// Name of storage entry -> Metadata of storage entry
    storage: HashMap<String, StorageMetadata>,
    /// Calls in the module, CallName -> encoded calls
    calls: HashMap<String, Vec<u8>>,
    events: HashMap<u8, ModuleEventMetadata>,
    // constants
}

impl ModuleMetadata {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// return the SCALE-encoded Call with parameters appended and parameters
    pub fn call<T: Encode>(
        &self, function: &'static str, params: T,
    ) -> Result<Encoded, MetadataError> {
        let fn_bytes = self
            .calls
            .get(function)
            .ok_or(MetadataError::CallNotFound(function))?;
        let mut bytes = vec![self.index];
        bytes.extend(fn_bytes);
        bytes.extend(params.encode());
        Ok(Encoded(bytes))
    }

    /// Return a storage entry by its key
    pub fn storage(&self, key: &'static str) -> Result<&StorageMetadata, MetadataError> {
        self.storage
            .get(key)
            .ok_or(MetadataError::StorageNotFound(key))
    }

    /// an iterator over all possible events for this module
    pub fn events(&self) -> impl Iterator<Item = &ModuleEventMetadata> {
        self.events.values()
    }

    // TODO Transfer to Subxt
    /// iterator over all possible calls in this module
    pub fn calls(&self) -> impl Iterator<Item = (&String, &Vec<u8>)> {
        self.calls.iter()
    }

    /// iterator over all storage keys in this module
    pub fn storage_keys(&self) -> impl Iterator<Item = (&String, &StorageMetadata)> {
        self.storage.iter()
    }

    /// get an event by its index in the module
    pub fn event(&self, index: u8) -> Result<&ModuleEventMetadata, MetadataError> {
        self.events
            .get(&index)
            .ok_or(MetadataError::EventNotFound(index))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StorageMetadata {
    prefix: String,
    modifier: StorageEntryModifier,
    ty: StorageEntryType,
    default: Vec<u8>,
    documentation: Vec<String>,
}

impl StorageMetadata {
    pub fn get_map<K: Encode, V: Decode + Clone>(
        &self,
    ) -> Result<StorageMap<K, V>, MetadataError> {
        match &self.ty {
            StorageEntryType::Map { hasher, .. } => {
                let prefix = self.prefix.as_bytes().to_vec();
                let hasher = hasher.to_owned();
                let default = Decode::decode(&mut &self.default[..])
                    .map_err(|_| MetadataError::MapValueTypeError)?;
                Ok(StorageMap {
                    _marker: PhantomData,
                    prefix,
                    hasher,
                    default,
                })
            }
            _ => Err(MetadataError::StorageTypeError),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StorageMap<K, V> {
    _marker: PhantomData<K>,
    prefix: Vec<u8>,
    hasher: StorageHasher,
    default: V,
}

impl<K: Encode, V: Decode + Clone> StorageMap<K, V> {
    pub fn key(&self, key: K) -> StorageKey {
        let mut bytes = self.prefix.clone();
        bytes.extend(key.encode());
        let hash = match self.hasher {
            StorageHasher::Blake2_128 => {
                substrate_primitives::blake2_128(&bytes).to_vec()
            }
            StorageHasher::Blake2_256 => {
                substrate_primitives::blake2_256(&bytes).to_vec()
            }
            StorageHasher::Blake2_128Concat => {
                substrate_primitives::blake2_128(&bytes).to_vec()
            }
            StorageHasher::Twox128 => substrate_primitives::twox_128(&bytes).to_vec(),
            StorageHasher::Twox256 => substrate_primitives::twox_256(&bytes).to_vec(),
            StorageHasher::Twox64Concat => substrate_primitives::twox_64(&bytes).to_vec(),
        };
        StorageKey(hash)
    }

    pub fn default(&self) -> V {
        self.default.clone()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleEventMetadata {
    pub name: String,
    pub(crate) arguments: HashSet<EventArg>,
}

impl ModuleEventMetadata {
    pub fn arguments(&self) -> Vec<EventArg> {
        self.arguments.iter().cloned().collect()
    }
}

/// Naive representation of event argument types, supports current set of
/// substrate EventArg types. If and when Substrate uses `type-metadata`, this
/// can be replaced.
///
/// Used to calculate the size of a instance of an event variant without having
/// the concrete type, so the raw bytes can be extracted from the encoded
/// `Vec<EventRecord<E>>` (without `E` defined).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum EventArg {
    Primitive(String),
    Vec(Box<EventArg>),
    Tuple(Vec<EventArg>),
}

impl FromStr for EventArg {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("Vec<") {
            if s.ends_with('>') {
                Ok(EventArg::Vec(Box::new(s[4 .. s.len() - 1].parse()?)))
            } else {
                Err(Error::InvalidEventArg(
                    s.to_string(),
                    "Expected closing `>` for `Vec`",
                ))
            }
        } else if s.starts_with("(") {
            if s.ends_with(")") {
                let mut args = Vec::new();
                for arg in s[1 .. s.len() - 1].split(',') {
                    let arg = arg.trim().parse()?;
                    args.push(arg)
                }
                Ok(EventArg::Tuple(args))
            } else {
                Err(Error::InvalidEventArg(
                    s.to_string(),
                    "Expecting closing `)` for tuple",
                ))
            }
        } else {
            Ok(EventArg::Primitive(s.to_string()))
        }
    }
}

impl EventArg {
    /// Returns all primitive types for this EventArg
    pub fn primitives(&self) -> Vec<String> {
        match self {
            EventArg::Primitive(p) => vec![p.clone()],
            EventArg::Vec(arg) => arg.primitives(),
            EventArg::Tuple(args) => {
                let mut primitives = Vec::new();
                for arg in args {
                    primitives.extend(arg.primitives())
                }
                primitives
            }
        }
    }
}

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "Invalid Prefix")]
    InvalidPrefix,
    #[fail(display = " Invalid Version")]
    InvalidVersion,
    #[fail(display = "Expected Decoded")]
    ExpectedDecoded,
    #[fail(display = "Invalid Event {}:{}", _0, _1)]
    InvalidEventArg(String, &'static str),
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::test_suite;

    #[test]
    fn should_create_metadata() {
        let meta = test_suite::runtime_v9();
        let meta: Metadata = Metadata::new(meta.as_slice());

        let meta = test_suite::runtime_v9_block6();
        let meta: Metadata = Metadata::new(meta.as_slice());
    }
}

