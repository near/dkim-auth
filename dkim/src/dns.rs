use crate::DKIMError;

/// A trait for entities that perform DNS resolution.
pub trait Lookup {
    fn lookup_txt(&self, name: &str) -> Result<Vec<String>, DKIMError>;
}
