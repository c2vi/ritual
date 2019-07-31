use crate::cpp_checks::CppChecks;
use crate::cpp_code_generator;
use crate::cpp_data::{CppItem, CppPath};
use crate::cpp_ffi_data::CppFfiItem;
use crate::rust_info::RustDatabaseItem;
use crate::rust_type::RustPath;
use log::{debug, info, trace};
use ritual_common::errors::{bail, format_err, Result};
use ritual_common::string_utils::ends_with_digit;
use ritual_common::target::LibraryTarget;
use serde_derive::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CppFfiDatabaseItem {
    pub id: FfiItemId,
    pub item: CppFfiItem,
    pub checks: CppChecks,
}

impl CppFfiDatabaseItem {
    pub fn path(&self) -> &CppPath {
        match &self.item {
            CppFfiItem::Function(f) => &f.path,
            CppFfiItem::QtSlotWrapper(s) => &s.class_path,
        }
    }

    pub fn is_source_item(&self) -> bool {
        match &self.item {
            CppFfiItem::Function(_) => false,
            CppFfiItem::QtSlotWrapper(_) => true,
        }
    }

    pub fn source_item_cpp_code(&self) -> Result<String> {
        match &self.item {
            CppFfiItem::Function(_) => bail!("not a source item"),
            CppFfiItem::QtSlotWrapper(slot_wrapper) => {
                cpp_code_generator::qt_slot_wrapper(slot_wrapper)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CppDatabaseItem {
    pub id: CppItemId,
    pub item: CppItem,
    pub source_ffi_item: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CppItemId(u32);

impl fmt::Display for CppItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl CppItemId {
    pub fn from_u32(value: u32) -> Self {
        CppItemId(value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FfiItemId(u32);

impl fmt::Display for FfiItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FfiItemId {
    pub fn from_u32(value: u32) -> Self {
        FfiItemId(value)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Data {
    crate_name: String,
    crate_version: String,
    cpp_items: Vec<CppDatabaseItem>,
    ffi_items: Vec<CppFfiDatabaseItem>,
    rust_items: Vec<RustDatabaseItem>,
    targets: Vec<LibraryTarget>,
    next_id: u32,
}

#[derive(Debug, Default)]
pub struct Counters {
    pub items_added: u32,
    pub items_ignored: u32,
}

/// Represents all collected data related to a crate.
#[derive(Debug)]
pub struct Database {
    data: Data,
    is_modified: bool,
    counters: Counters,
}

impl Database {
    pub fn new(data: Data) -> Database {
        Database {
            data,
            is_modified: false,
            counters: Counters::default(),
        }
    }

    pub fn data(&self) -> &Data {
        &self.data
    }

    pub fn empty(crate_name: impl Into<String>) -> Self {
        let crate_name = crate_name.into();
        Database {
            data: Data {
                crate_name: crate_name.clone(),
                crate_version: "0.0.0".into(),
                cpp_items: Vec::new(),
                ffi_items: Vec::new(),
                rust_items: Vec::new(),
                targets: Vec::new(),
                next_id: 1,
            },
            is_modified: true,
            counters: Counters::default(),
        }
    }

    pub fn is_modified(&self) -> bool {
        self.is_modified
    }

    pub fn set_saved(&mut self) {
        self.is_modified = false;
    }

    pub fn cpp_items(&self) -> &[CppDatabaseItem] {
        &self.data.cpp_items
    }

    pub fn cpp_item_ids<'a>(&'a self) -> impl Iterator<Item = CppItemId> + 'a {
        self.data.cpp_items.iter().map(|item| item.id)
    }

    pub fn cpp_items_mut(&mut self) -> &mut [CppDatabaseItem] {
        self.is_modified = true;
        &mut self.data.cpp_items
    }

    pub fn cpp_item(&self, id: CppItemId) -> Result<&CppDatabaseItem> {
        match self
            .data
            .cpp_items
            .binary_search_by_key(&id, |item| item.id)
        {
            Ok(index) => Ok(&self.data.cpp_items[index]),
            Err(_) => bail!("invalid cpp item id: {}", id),
        }
    }

    pub fn cpp_item_mut(&mut self, id: CppItemId) -> Result<&mut CppDatabaseItem> {
        match self
            .data
            .cpp_items
            .binary_search_by_key(&id, |item| item.id)
        {
            Ok(index) => Ok(&mut self.data.cpp_items[index]),
            Err(_) => bail!("invalid cpp item id: {}", id),
        }
    }

    pub fn ffi_items(&self) -> &[CppFfiDatabaseItem] {
        &self.data.ffi_items
    }

    pub fn ffi_items_mut(&mut self) -> &mut [CppFfiDatabaseItem] {
        self.is_modified = true;
        &mut self.data.ffi_items
    }

    pub fn add_ffi_item(&mut self, item: CppFfiItem) -> bool {
        self.is_modified = true;
        if self
            .data
            .ffi_items
            .iter()
            .any(|i| i.item.has_same_source(&item))
        {
            self.counters.items_ignored += 1;
            return false;
        }

        let id = FfiItemId(self.data.next_id);
        self.data.next_id += 1;

        self.data.ffi_items.push(CppFfiDatabaseItem {
            id,
            item,
            checks: CppChecks::default(),
        });
        self.counters.items_added += 1;
        true
    }

    pub fn clear(&mut self) {
        self.is_modified = true;
        self.data.cpp_items.clear();
        self.data.targets.clear();
    }

    pub fn clear_ffi(&mut self) {
        self.is_modified = true;
        self.data.ffi_items.clear();
        self.data
            .cpp_items
            .retain(|item| item.source_ffi_item.is_none());
        // TODO: deal with rust items that now have invalid index references
    }

    pub fn clear_cpp_checks(&mut self) {
        self.is_modified = true;
        for item in &mut self.data.ffi_items {
            item.checks.clear();
        }
    }

    pub fn crate_name(&self) -> &str {
        &self.data.crate_name
    }

    pub fn crate_version(&self) -> &str {
        &self.data.crate_version
    }

    pub fn set_crate_version(&mut self, version: String) {
        if self.data.crate_version != version {
            self.is_modified = true;
            self.data.crate_version = version;
        }
    }

    pub fn add_cpp_item(
        &mut self,
        source_ffi_item: Option<usize>,
        data: CppItem,
    ) -> Option<CppItemId> {
        if self
            .data
            .cpp_items
            .iter_mut()
            .any(|item| item.item.is_same(&data))
        {
            self.counters.items_ignored += 1;
            return None;
        }
        self.is_modified = true;
        let id = CppItemId(self.data.next_id);
        self.data.next_id += 1;
        debug!("added cpp item #{}: {}", id, data);
        let item = CppDatabaseItem {
            id,
            item: data,
            source_ffi_item,
        };
        trace!("cpp item data: {:?}", item);
        self.data.cpp_items.push(item);
        self.counters.items_added += 1;
        Some(id)
    }

    pub fn clear_rust_info(&mut self) {
        self.is_modified = true;
        self.data.rust_items.clear();
    }

    pub fn add_environment(&mut self, env: LibraryTarget) {
        if !self.data.targets.iter().any(|e| e == &env) {
            self.is_modified = true;
            self.data.targets.push(env.clone());
        }
    }

    pub fn environments(&self) -> &[LibraryTarget] {
        &self.data.targets
    }

    pub fn find_rust_item(&self, path: &RustPath) -> Option<&RustDatabaseItem> {
        self.data
            .rust_items
            .iter()
            .find(|item| item.path() == Some(path))
    }

    pub fn rust_children<'a>(
        &'a self,
        path: &'a RustPath,
    ) -> impl Iterator<Item = &'a RustDatabaseItem> {
        self.data
            .rust_items
            .iter()
            .filter(move |item| item.is_child_of(path))
    }

    pub fn rust_items(&self) -> &[RustDatabaseItem] {
        &self.data.rust_items
    }

    pub fn add_rust_item(&mut self, item: RustDatabaseItem) -> Result<()> {
        self.is_modified = true;
        if item.item.is_crate_root() {
            let item_path = item.path().expect("crate root must have path");
            let crate_name = item_path
                .crate_name()
                .expect("rust item path must have crate name");
            if crate_name != self.data.crate_name {
                bail!("can't add rust item with different crate name: {:?}", item);
            }
        } else {
            let mut path = item
                .parent_path()
                .map_err(|_| format_err!("path has no parent for rust item: {:?}", item))?;
            let crate_name = path
                .crate_name()
                .expect("rust item path must have crate name");
            if crate_name != self.data.crate_name {
                bail!("can't add rust item with different crate name: {:?}", item);
            }
            while path.parts.len() > 1 {
                if self.find_rust_item(&path).is_none() {
                    bail!("unreachable path {:?} for rust item: {:?}", path, item);
                }
                path.parts.pop();
            }
        }

        if self
            .data
            .rust_items
            .iter()
            .any(|other| other.item.has_same_source(&item.item))
        {
            self.counters.items_ignored += 1;
            return Ok(());
        }

        self.data.rust_items.push(item);
        self.counters.items_added += 1;
        Ok(())
    }

    pub fn make_unique_rust_path(&self, path: &RustPath) -> RustPath {
        let mut number = None;
        let mut path_try = path.clone();
        loop {
            if let Some(number) = number {
                *path_try.last_mut() = format!(
                    "{}{}{}",
                    path.last(),
                    if ends_with_digit(path.last()) {
                        "_"
                    } else {
                        ""
                    },
                    number
                );
            }
            if self.find_rust_item(&path_try).is_none() {
                return path_try;
            }

            number = Some(number.unwrap_or(1) + 1);
        }
        // TODO: check for conflicts with types from crate template (how?)
    }

    pub fn report_counters(&mut self) {
        if self.counters.items_added > 0 || self.counters.items_ignored > 0 {
            if self.counters.items_ignored == 0 {
                info!("Items added: {}", self.counters.items_added);
            } else {
                info!(
                    "Items added: {}, ignored: {}",
                    self.counters.items_added, self.counters.items_ignored
                );
            }
        }
        self.counters = Counters::default();
    }
}
