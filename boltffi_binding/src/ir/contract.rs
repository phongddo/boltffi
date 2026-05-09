use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::{BindingError, BindingErrorKind, CanonicalName, Decl, NativeSymbolTable};

/// Schema marker carried in every serialized binding contract.
///
/// The major component changes when the schema becomes incompatible: code
/// compiled against an older major cannot make sense of the new bytes. The
/// minor component grows additively for fields older readers can safely
/// ignore. [`readable`](Self::readable) is the rule both halves enforce
/// together.
///
/// # Example
///
/// `ContractVersion::new(1, 3)` is readable by code built against
/// `CURRENT = (1, 5)`. `ContractVersion::new(2, 0)` is not, because the
/// major component disagrees.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ContractVersion {
    major: u16,
    minor: u16,
}

impl ContractVersion {
    /// Version written by this crate.
    pub const CURRENT: Self = Self { major: 0, minor: 1 };

    /// Returns [`Self::CURRENT`].
    pub const fn current() -> Self {
        Self::CURRENT
    }

    /// Builds a version from its components.
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Returns the major component.
    pub const fn major(self) -> u16 {
        self.major
    }

    /// Returns the minor component.
    pub const fn minor(self) -> u16 {
        self.minor
    }

    /// Returns `true` when the major matches [`Self::CURRENT`] and the minor
    /// is no greater than [`Self::CURRENT`].
    pub const fn readable(self) -> bool {
        self.major == Self::CURRENT.major && self.minor <= Self::CURRENT.minor
    }
}

/// The Rust package whose API a [`Bindings`] describes.
///
/// The name is the source-of-truth identifier that generated module names,
/// diagnostics, and on-disk artifacts refer back to. The version is the
/// `Cargo.toml` value when present and exists for human-readable messages;
/// it is not part of contract identity.
///
/// # Example
///
/// A `Cargo.toml` with `name = "demo"` and `version = "0.2.1"` produces a
/// `PackageInfo` whose name canonicalizes to `["demo"]` and whose version
/// is `Some("0.2.1")`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct PackageInfo {
    name: CanonicalName,
    version: Option<String>,
}

impl PackageInfo {
    pub(crate) fn new(name: CanonicalName, version: Option<String>) -> Self {
        Self { name, version }
    }

    /// Returns the package name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the package version.
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
}

/// The complete classified API of one Rust crate.
///
/// Holds every record, enum, function, class, callback, stream, constant,
/// and custom type the user exported, paired with the FFI decision the
/// classifier made for it. The native symbol table lists every linker
/// name the bindings will call. The schema version states which release of
/// this crate produced the contract.
///
/// A `Bindings` is always valid by construction. Pattern matching cannot
/// witness duplicate ids, an unreadable schema version, or a symbol table
/// with inconsistent entries; the crate exposes no fallible accessor that
/// would hand back a partially constructed value.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Bindings {
    version: ContractVersion,
    package: PackageInfo,
    decls: Vec<Decl>,
    symbols: NativeSymbolTable,
}

impl Bindings {
    pub(crate) fn new(
        package: PackageInfo,
        decls: Vec<Decl>,
        symbols: NativeSymbolTable,
    ) -> Result<Self, BindingError> {
        let bindings = Self {
            version: ContractVersion::current(),
            package,
            decls,
            symbols,
        };
        bindings.validate()?;
        Ok(bindings)
    }

    /// Returns the schema version.
    pub const fn version(&self) -> ContractVersion {
        self.version
    }

    /// Returns the producing package.
    pub fn package(&self) -> &PackageInfo {
        &self.package
    }

    /// Returns the declarations.
    pub fn decls(&self) -> &[Decl] {
        &self.decls
    }

    /// Returns the native symbol table.
    pub fn symbols(&self) -> &NativeSymbolTable {
        &self.symbols
    }

    /// Returns `true` when [`Self::version`] is readable by this crate.
    pub const fn readable(&self) -> bool {
        self.version.readable()
    }

    /// Returns `Ok` when:
    ///
    /// - the contract version is readable by this crate;
    /// - every native symbol has a callable spelling and a unique id and name;
    /// - every declaration id is unique within its family.
    ///
    /// Returns the first failed invariant otherwise.
    pub fn validate(&self) -> Result<(), BindingError> {
        validate_contract_version(self.version)?;
        self.symbols.validate()?;
        validate_unique_decl_ids(&self.decls)
    }
}

fn validate_contract_version(version: ContractVersion) -> Result<(), BindingError> {
    if version.readable() {
        Ok(())
    } else {
        Err(BindingError::new(BindingErrorKind::UnsupportedVersion {
            actual: version,
            current: ContractVersion::current(),
        }))
    }
}

fn validate_unique_decl_ids(decls: &[Decl]) -> Result<(), BindingError> {
    decls
        .iter()
        .map(Decl::id)
        .try_fold(HashSet::new(), |mut seen, decl_id| {
            if seen.insert(decl_id) {
                Ok(seen)
            } else {
                Err(BindingError::new(BindingErrorKind::DuplicateDeclarationId(
                    decl_id,
                )))
            }
        })
        .map(|_| ())
}
