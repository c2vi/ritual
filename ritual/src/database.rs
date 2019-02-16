use crate::cpp_data::CppBaseSpecifier;
use crate::cpp_data::CppClassField;
use crate::cpp_data::CppEnumValue;
use crate::cpp_data::CppOriginLocation;
use crate::cpp_data::CppTypeData;
use crate::cpp_data::CppTypeDataKind;
use ritual_common::target::Target;

use crate::cpp_data::CppVisibility;
use crate::cpp_function::CppFunction;

use crate::cpp_data::CppPath;
use crate::cpp_ffi_data::CppFfiFunction;
use crate::cpp_ffi_data::QtSlotWrapper;
use crate::cpp_type::CppType;
use crate::rust_info::RustDatabase;
use itertools::Itertools;
use serde_derive::{Deserialize, Serialize};
use std::fmt::Display;
use std::fmt::Formatter;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CppCheckerEnv {
    pub target: Target,
    pub cpp_library_version: Option<String>,
}

impl CppCheckerEnv {
    pub fn short_text(&self) -> String {
        format!(
            "{}/{:?}-{:?}-{:?}-{:?}",
            self.cpp_library_version
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or("None"),
            self.target.arch,
            self.target.os,
            self.target.family,
            self.target.env
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatabaseItemSource {
    CppParser {
        /// File name of the include file (without full path)
        include_file: String,
        /// Exact location of the declaration
        origin_location: CppOriginLocation,
    },
    ImplicitDestructor,
    TemplateInstantiation,
    NamespaceInfering,
    QtSignalArguments,
}

impl DatabaseItemSource {
    pub fn is_parser(&self) -> bool {
        match *self {
            DatabaseItemSource::CppParser { .. } => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CppCheckerInfo {
    pub env: CppCheckerEnv,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CppCheckerInfoList {
    pub items: Vec<CppCheckerInfo>,
}

pub enum CppCheckerAddResult {
    Added,
    Changed { old: Option<String> },
    Unchanged,
}

impl CppCheckerInfoList {
    pub fn add(&mut self, env: &CppCheckerEnv, error: Option<String>) -> CppCheckerAddResult {
        if let Some(item) = self.items.iter_mut().find(|i| &i.env == env) {
            let r = if item.error == error {
                CppCheckerAddResult::Unchanged
            } else {
                CppCheckerAddResult::Changed {
                    old: item.error.clone(),
                }
            };
            item.error = error;
            return r;
        }
        self.items.push(CppCheckerInfo {
            env: env.clone(),
            error,
        });
        CppCheckerAddResult::Added
    }

    pub fn any_passed(&self) -> bool {
        self.items.iter().any(|check| check.error.is_none())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum CppItemData {
    Namespace(CppPath),
    Type(CppTypeData),
    EnumValue(CppEnumValue),
    Function(CppFunction),
    ClassField(CppClassField),
    ClassBase(CppBaseSpecifier),
    QtSignalArguments(Vec<CppType>),
}

impl CppItemData {
    pub fn is_same(&self, other: &CppItemData) -> bool {
        use self::CppItemData::*;

        match *self {
            Namespace(ref v) => {
                if let Namespace(ref v2) = other {
                    v == v2
                } else {
                    false
                }
            }
            Type(ref v) => {
                if let Type(ref v2) = other {
                    v.is_same(v2)
                } else {
                    false
                }
            }
            EnumValue(ref v) => {
                if let EnumValue(ref v2) = other {
                    v.is_same(v2)
                } else {
                    false
                }
            }
            Function(ref v) => {
                if let Function(ref v2) = other {
                    v.is_same(v2)
                } else {
                    false
                }
            }
            ClassField(ref v) => {
                if let ClassField(ref v2) = other {
                    v.is_same(v2)
                } else {
                    false
                }
            }
            ClassBase(ref v) => {
                if let ClassBase(ref v2) = other {
                    v == v2
                } else {
                    false
                }
            }
            QtSignalArguments(ref v) => {
                if let QtSignalArguments(ref v2) = other {
                    v == v2
                } else {
                    false
                }
            }
        }
    }

    pub fn path(&self) -> Option<&CppPath> {
        let path = match self {
            CppItemData::Namespace(data) => data,
            CppItemData::Type(data) => &data.path,
            CppItemData::EnumValue(data) => &data.path,
            CppItemData::Function(data) => &data.path,
            CppItemData::ClassField(data) => &data.path,
            CppItemData::ClassBase(_) | CppItemData::QtSignalArguments(_) => return None,
        };
        Some(path)
    }

    pub fn all_involved_types(&self) -> Vec<CppType> {
        match *self {
            CppItemData::Type(ref t) => match t.kind {
                CppTypeDataKind::Enum => vec![CppType::Enum {
                    path: t.path.clone(),
                }],
                CppTypeDataKind::Class { .. } => vec![CppType::Class(t.path.clone())],
            },
            CppItemData::EnumValue(ref enum_value) => vec![CppType::Enum {
                path: enum_value
                    .path
                    .parent()
                    .expect("enum value must have parent path"),
            }],
            CppItemData::Namespace(_) => Vec::new(),
            CppItemData::Function(ref function) => function.all_involved_types(),
            CppItemData::ClassField(ref field) => {
                let class_type =
                    CppType::Class(field.path.parent().expect("field path must have parent"));
                vec![class_type, field.field_type.clone()]
            }
            CppItemData::ClassBase(ref base) => vec![
                CppType::Class(base.base_class_type.clone()),
                CppType::Class(base.derived_class_type.clone()),
            ],
            CppItemData::QtSignalArguments(ref args) => args.clone(),
        }
    }

    pub fn as_namespace_ref(&self) -> Option<&CppPath> {
        if let CppItemData::Namespace(ref data) = *self {
            Some(data)
        } else {
            None
        }
    }
    pub fn as_function_ref(&self) -> Option<&CppFunction> {
        if let CppItemData::Function(ref data) = *self {
            Some(data)
        } else {
            None
        }
    }
    pub fn as_field_ref(&self) -> Option<&CppClassField> {
        if let CppItemData::ClassField(ref data) = *self {
            Some(data)
        } else {
            None
        }
    }
    pub fn as_enum_value_ref(&self) -> Option<&CppEnumValue> {
        if let CppItemData::EnumValue(ref data) = *self {
            Some(data)
        } else {
            None
        }
    }
    pub fn as_base_ref(&self) -> Option<&CppBaseSpecifier> {
        if let CppItemData::ClassBase(ref data) = *self {
            Some(data)
        } else {
            None
        }
    }
    pub fn as_type_ref(&self) -> Option<&CppTypeData> {
        if let CppItemData::Type(ref data) = *self {
            Some(data)
        } else {
            None
        }
    }
    pub fn as_type_mut(&mut self) -> Option<&mut CppTypeData> {
        if let CppItemData::Type(ref mut data) = *self {
            Some(data)
        } else {
            None
        }
    }

    pub fn as_signal_arguments_ref(&self) -> Option<&[CppType]> {
        if let CppItemData::QtSignalArguments(ref data) = *self {
            Some(data)
        } else {
            None
        }
    }

    /*pub fn path(&self) -> Option<String> {
        unimplemented!()
    }*/
}

impl Display for CppItemData {
    fn fmt(&self, f: &mut Formatter) -> ::std::result::Result<(), ::std::fmt::Error> {
        let s = match *self {
            CppItemData::Namespace(ref path) => format!("namespace {}", path.to_cpp_pseudo_code()),
            CppItemData::Type(ref type1) => match type1.kind {
                CppTypeDataKind::Enum => format!("enum {}", type1.path.to_cpp_pseudo_code()),
                CppTypeDataKind::Class { .. } => {
                    format!("class {}", type1.path.to_cpp_pseudo_code())
                }
            },
            CppItemData::Function(ref method) => method.short_text(),
            CppItemData::EnumValue(ref value) => format!(
                "enum value {} = {}",
                value.path.to_cpp_pseudo_code(),
                value.value
            ),
            CppItemData::ClassField(ref field) => field.short_text(),
            CppItemData::ClassBase(ref class_base) => {
                let virtual_text = if class_base.is_virtual {
                    "virtual "
                } else {
                    ""
                };
                let visibility_text = match class_base.visibility {
                    CppVisibility::Public => "public",
                    CppVisibility::Protected => "protected",
                    CppVisibility::Private => "private",
                };
                let index_text = if class_base.base_index > 0 {
                    format!(" (index: {}", class_base.base_index)
                } else {
                    String::new()
                };
                format!(
                    "class {} : {}{} {}{}",
                    class_base.derived_class_type.to_cpp_pseudo_code(),
                    virtual_text,
                    visibility_text,
                    class_base.base_class_type.to_cpp_pseudo_code(),
                    index_text
                )
            }
            CppItemData::QtSignalArguments(ref args) => format!(
                "Qt signal args ({})",
                args.iter().map(|arg| arg.to_cpp_pseudo_code()).join(", ")
            ),
        };

        f.write_str(&s)
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CppFfiItemKind {
    Function(CppFfiFunction),

    // TODO: separate custom C++ wrapper logic from core implementation,
    // run cpp_parser on wrappers instead of constructing results manually
    QtSlotWrapper(QtSlotWrapper),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CppFfiItem {
    pub kind: CppFfiItemKind,
    pub checks: CppCheckerInfoList,
    pub is_rust_processed: bool,
}

impl CppFfiItem {
    pub fn from_function(function: CppFfiFunction) -> Self {
        CppFfiItem {
            kind: CppFfiItemKind::Function(function),
            checks: Default::default(),
            is_rust_processed: false,
        }
    }

    pub fn from_qt_slot_wrapper(wrapper: QtSlotWrapper) -> Self {
        CppFfiItem {
            kind: CppFfiItemKind::QtSlotWrapper(wrapper),
            checks: Default::default(),
            is_rust_processed: false,
        }
    }

    pub fn path(&self) -> &CppPath {
        match self.kind {
            CppFfiItemKind::Function(ref f) => &f.path,
            CppFfiItemKind::QtSlotWrapper(ref s) => &s.class_path,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CppDatabaseItem {
    pub cpp_data: CppItemData,

    pub source: DatabaseItemSource,
    pub ffi_items: Vec<CppFfiItem>,
    pub is_cpp_ffi_processed: bool,
    pub is_rust_processed: bool,
}

impl CppDatabaseItem {
    pub fn is_all_rust_processed(&self) -> bool {
        self.is_rust_processed && self.ffi_items.iter().all(|m| m.is_rust_processed)
    }
}

/// Represents all collected data related to a crate.
#[derive(Debug, Serialize, Deserialize)]
pub struct Database {
    pub crate_name: String,
    pub crate_version: String,
    pub cpp_items: Vec<CppDatabaseItem>,
    pub rust_database: RustDatabase,
    pub environments: Vec<CppCheckerEnv>,
}

impl Database {
    pub fn empty(crate_name: impl Into<String>) -> Database {
        Database {
            crate_name: crate_name.into(),
            crate_version: "0.0.0".into(),
            cpp_items: Vec::new(),
            rust_database: Default::default(),
            environments: Vec::new(),
        }
    }

    pub fn items(&self) -> &[CppDatabaseItem] {
        &self.cpp_items
    }

    pub fn clear(&mut self) {
        self.cpp_items.clear();
        self.environments.clear();
    }

    pub fn crate_name(&self) -> &str {
        &self.crate_name
    }

    pub fn add_cpp_data(&mut self, source: DatabaseItemSource, data: CppItemData) -> bool {
        if let Some(item) = self
            .cpp_items
            .iter_mut()
            .find(|item| item.cpp_data.is_same(&data))
        {
            // parser data takes priority
            if source.is_parser() && !item.source.is_parser() {
                item.source = source;
            }
            return false;
        }
        self.cpp_items.push(CppDatabaseItem {
            cpp_data: data,
            source,
            ffi_items: Vec::new(),
            is_cpp_ffi_processed: false,
            is_rust_processed: false,
        });
        true
    }

    /*
    pub fn mark_missing_cpp_data(&mut self, env: DataEnv) {
      let info = DataEnvInfo {
        is_success: false,
        ..DataEnvInfo::default()
      };
      for item in &mut self.items {
        if !item.environments.iter().any(|env2| env2.env == env) {
          item.environments.push(DataEnvWithInfo {
            env: env.clone(),
            info: info.clone(),
          });
        }
      }
    }*/
}
