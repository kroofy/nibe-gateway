use nibe_core::{
    RegisterAccess, RegisterDefinition, RegisterType, ServiceError, VerificationState,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use utoipa::ToSchema;

pub const BUNDLED_CATALOG: &str = include_str!("../../../fixtures/register-catalogs/f730.json");
pub const MAX_CATALOG_BYTES: usize = 2 * 1024 * 1024;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub enum DataType {
    #[serde(rename = "u8")]
    U8,
    #[serde(rename = "s8")]
    S8,
    #[serde(rename = "u16")]
    U16,
    #[serde(rename = "s16")]
    S16,
    #[serde(rename = "u32")]
    U32,
    #[serde(rename = "s32")]
    S32,
}
impl DataType {
    pub fn width(self) -> usize {
        match self {
            Self::U8 | Self::S8 => 1,
            Self::U16 | Self::S16 => 2,
            _ => 4,
        }
    }
    pub fn core_type(self) -> RegisterType {
        match self {
            Self::U8 => RegisterType::U8,
            Self::S8 => RegisterType::I8,
            Self::U16 => RegisterType::U16,
            Self::S16 => RegisterType::I16,
            Self::U32 => RegisterType::U32,
            Self::S32 => RegisterType::I32,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Entry {
    pub id: u16,
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub data_type: DataType,
    pub scale: u32,
    pub unit: Option<String>,
    pub access: String,
    pub write_allowed: bool,
    pub raw_min: Option<i64>,
    pub raw_max: Option<i64>,
    pub raw_default: Option<i64>,
    pub value_map: Option<BTreeMap<String, String>>,
    pub flags: Vec<String>,
    pub sources: Vec<String>,
    pub conflicts: Vec<serde_json::Value>,
    pub notes: Vec<String>,
    pub verification: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Catalog {
    pub schema: String,
    pub schema_version: u32,
    pub catalog_id: String,
    pub catalog_version: String,
    pub model: String,
    pub write_policy: String,
    pub sources: BTreeMap<String, serde_json::Value>,
    pub registers: Vec<Entry>,
    #[serde(flatten)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}
impl Catalog {
    pub fn bundled() -> Self {
        Self::load(BUNDLED_CATALOG.as_bytes()).expect("checked-in F730 catalog")
    }
    pub fn load(bytes: &[u8]) -> Result<Self, ServiceError> {
        if bytes.len() > MAX_CATALOG_BYTES {
            return Err(ServiceError::validation("catalog exceeds 2 MiB"));
        }
        let catalog: Self = serde_json::from_slice(bytes)
            .map_err(|e| ServiceError::validation(format!("invalid catalog: {e}")))?;
        catalog.validate()?;
        Ok(catalog)
    }
    pub fn validate(&self) -> Result<(), ServiceError> {
        let invalid = |s| Err(ServiceError::validation(s));
        if self.schema != "nibe-gateway/register-catalog"
            || self.schema_version != 1
            || self.model != "F730"
            || self.write_policy != "disabled"
        {
            return invalid("requires F730 schema version 1 and disabled write policy");
        }
        if self.registers.is_empty()
            || self.registers.len() > 4096
            || self.catalog_version.is_empty()
        {
            return invalid("invalid catalog size/version");
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for e in &self.registers {
            if !ids.insert(e.id)
                || !names.insert(&e.name)
                || e.id == u16::MAX
                || e.name.is_empty()
                || e.scale == 0
                || e.scale > 1_000_000
                || e.write_allowed
                || !matches!(e.access.as_str(), "read" | "read_write")
                || e.sources.is_empty()
                || e.sources.iter().any(|s| !self.sources.contains_key(s))
                || e.verification != "unverified"
                || matches!((e.raw_min,e.raw_max),(Some(a),Some(b)) if a>b)
            {
                return invalid(
                    "invalid/duplicate register, source, scale, verification, or write permission",
                );
            }
        }
        Ok(())
    }
    pub fn entry(&self, id: u16) -> Option<&Entry> {
        self.registers.iter().find(|e| e.id == id)
    }
    pub fn definitions(&self) -> Vec<RegisterDefinition> {
        self.registers
            .iter()
            .map(|e| RegisterDefinition {
                id: e.id.into(),
                name: e.name.clone(),
                description: e.description.clone().unwrap_or_else(|| e.title.clone()),
                data_type: e.data_type.core_type(),
                scale: 1.0 / f64::from(e.scale),
                unit: e.unit.clone(),
                access: if e.access == "read" {
                    RegisterAccess::ReadOnly
                } else {
                    RegisterAccess::ReadWrite
                },
                verification: VerificationState::Unverified,
                models: vec![self.model.clone()],
                firmware: None,
                source: e.sources.join(", "),
                catalog_version: self.catalog_version.clone(),
            })
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_real_catalog_and_refuses_unsafe_imports() {
        let c = Catalog::bundled();
        assert_eq!(c.registers.len(), 581);
        assert_eq!(c.entry(40004).unwrap().scale, 10);
        assert!(c
            .entry(48852)
            .unwrap()
            .flags
            .contains(&"never_write".into()));
        let mut c = c.clone();
        c.registers[0].write_allowed = true;
        assert!(c.validate().is_err());
        c.registers[0].write_allowed = false;
        c.registers[0].scale = 0;
        assert!(c.validate().is_err());
        assert!(Catalog::load(b"{}").is_err());
    }
}
