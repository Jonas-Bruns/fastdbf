use crate::error::{Error, Result};
use crate::header::FieldDescriptor;
use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    deleted: bool,
    values: Vec<Value>,
}

impl Record {
    pub fn new(fields: &[FieldDescriptor]) -> Self {
        Self {
            deleted: false,
            values: fields.iter().map(FieldDescriptor::empty_value).collect(),
        }
    }

    pub fn from_values(deleted: bool, values: Vec<Value>) -> Self {
        Self { deleted, values }
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted
    }

    pub fn set_deleted(&mut self, deleted: bool) {
        self.deleted = deleted;
    }

    pub fn values(&self) -> &[Value] {
        &self.values
    }

    pub fn values_mut(&mut self) -> &mut [Value] {
        &mut self.values
    }

    pub fn get(&self, fields: &[FieldDescriptor], name: &str) -> Result<&Value> {
        let index = index_of(fields, name)?;
        Ok(&self.values[index])
    }

    pub fn get_mut(&mut self, fields: &[FieldDescriptor], name: &str) -> Result<&mut Value> {
        let index = index_of(fields, name)?;
        Ok(&mut self.values[index])
    }

    pub fn insert(&mut self, fields: &[FieldDescriptor], name: &str, value: Value) -> Result<()> {
        let index = index_of(fields, name)?;
        self.values[index] = value;
        Ok(())
    }
}

fn index_of(fields: &[FieldDescriptor], name: &str) -> Result<usize> {
    let normalized = name.trim().to_ascii_uppercase();
    fields
        .iter()
        .position(|field| field.name == normalized)
        .ok_or(Error::FieldNotFound(normalized))
}
