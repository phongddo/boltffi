use std::{error::Error, fmt};

use crate::{ContractVersion, DeclarationId, SymbolId};

/// A reason a binding contract could not be exposed.
///
/// Returned only at construction boundaries: deserialization, symbol-table
/// building, validation. Once a [`Bindings`](crate::Bindings) value is
/// held, the failures listed in [`BindingErrorKind`] cannot occur.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BindingError {
    kind: BindingErrorKind,
}

impl BindingError {
    /// Builds an error from a specific failure.
    pub fn new(kind: BindingErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the failure that produced this error.
    pub fn kind(&self) -> &BindingErrorKind {
        &self.kind
    }
}

impl fmt::Display for BindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            BindingErrorKind::UnsupportedVersion { actual, current } => write!(
                formatter,
                "binding contract version {}.{} cannot be read by {}.{}",
                actual.major(),
                actual.minor(),
                current.major(),
                current.minor()
            ),
            BindingErrorKind::DuplicateDeclarationId(id) => {
                write!(formatter, "duplicate declaration id {id:?}")
            }
            BindingErrorKind::DuplicateSymbolId(id) => {
                write!(formatter, "duplicate native symbol id {id:?}")
            }
            BindingErrorKind::DuplicateSymbolName(name) => {
                write!(formatter, "duplicate native symbol name `{name}`")
            }
            BindingErrorKind::InvalidSymbolName(name) => {
                write!(formatter, "invalid native symbol name `{name}`")
            }
        }
    }
}

impl Error for BindingError {}

/// The specific failure that produced a [`BindingError`].
///
/// Listed exhaustively so callers can pattern match and produce a targeted
/// diagnostic for each kind of contract problem.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum BindingErrorKind {
    /// The contract was written with a schema this crate cannot read.
    UnsupportedVersion {
        /// Version found in the serialized contract.
        actual: ContractVersion,
        /// Highest version this crate understands.
        current: ContractVersion,
    },
    /// Two top-level declarations share the same id.
    DuplicateDeclarationId(DeclarationId),
    /// Two native symbols share the same id.
    DuplicateSymbolId(SymbolId),
    /// Two native symbols share the same exported name.
    DuplicateSymbolName(String),
    /// A native symbol name is empty or not a valid C identifier.
    InvalidSymbolName(String),
}
