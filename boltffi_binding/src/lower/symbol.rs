//! Native symbol minting and id allocation for the lowering pass.
//!
//! Every callable the lowered IR exposes references one or more native
//! symbols by id. Ids are sequential integers assigned in the order the
//! pass mints them; names follow the boltffi convention shared with the
//! foreign-side bindings: `boltffi_<owner_snake>_<member>` for owned
//! callables, `boltffi_<owner_snake>_new` for the canonical "new"
//! initializer. The convention used to live in `boltffi_ffi_rules` and
//! is now anchored here, where the lowering pass owns it.
//!
//! The owner name passed in is the Rust-side type identifier
//! (`MyRecord`); this module snake-cases it for use in the symbol.

use crate::{NativeSymbol, SymbolId, SymbolName};

use super::LowerError;

/// Symbol prefix shared by every binding the contract exposes.
pub(super) const FFI_PREFIX: &str = "boltffi";

/// Hands out [`SymbolId`]s in the order callers mint native symbols.
///
/// Ids are stable inside one [`crate::Bindings`](crate::Bindings) value
/// but carry no meaning outside it; their job is to keep equal symbols
/// equal across the contract's symbol table.
pub(super) struct SymbolAllocator {
    next: u32,
}

impl SymbolAllocator {
    pub(super) fn new() -> Self {
        Self { next: 0 }
    }

    /// Mints a [`NativeSymbol`] from a constructed FFI name, allocating
    /// a fresh [`SymbolId`].
    pub(super) fn mint(&mut self, name: String) -> Result<NativeSymbol, LowerError> {
        let id = self.next_id();
        let parsed = SymbolName::parse(name)?;
        Ok(NativeSymbol::new(id, parsed))
    }

    fn next_id(&mut self) -> SymbolId {
        let id = SymbolId::from_raw(self.next);
        self.next += 1;
        id
    }
}

/// Builds the symbol used for a named callable owned by `owner_name`.
///
/// `owner_name` is the type identifier (`MyRecord`); `member_name` is
/// the method or initializer name. The leading FFI prefix and the
/// snake-cased owner name come from this module.
pub(super) fn member_symbol_name(owner_name: &str, member_name: &str) -> String {
    format!(
        "{}_{}_{}",
        FFI_PREFIX,
        to_snake_case(owner_name),
        member_name
    )
}

/// Builds the symbol used for the canonical "new" initializer.
///
/// Returned name has the form `boltffi_<owner_snake>_new`. Used for
/// the initializer the binding generators call by convention.
pub(super) fn canonical_new_symbol_name(owner_name: &str) -> String {
    format!("{}_{}_new", FFI_PREFIX, to_snake_case(owner_name))
}

/// Lowercases `name` and inserts an underscore at every word boundary.
///
/// Word boundaries are:
///
/// - A lowercase or digit followed by an uppercase character, e.g.
///   `MyRecord` → `my_record`.
/// - An uppercase character at the end of an acronym, identified by
///   the next character being lowercase, e.g. `HTTPHeader` →
///   `http_header`, `XMLParser` → `xml_parser`.
///
/// Pure runs of uppercase characters (`HTTP`) collapse to lowercase
/// without internal underscores. Strings that already use snake_case
/// pass through unchanged.
pub(super) fn to_snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let initial = String::with_capacity(name.len() + chars.len() / 2);
    chars
        .iter()
        .enumerate()
        .fold(initial, |mut result, (index, &character)| {
            if character.is_uppercase() && index > 0 {
                let previous = chars[index - 1];
                let next = chars.get(index + 1).copied();
                let previous_is_word = previous.is_lowercase() || previous.is_ascii_digit();
                let acronym_word_break = previous.is_uppercase()
                    && next.is_some_and(|character| character.is_lowercase());
                if previous_is_word || acronym_word_break {
                    result.push('_');
                }
            }
            result.extend(character.to_lowercase());
            result
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_lowercases_camel_words() {
        assert_eq!(to_snake_case("MyRecord"), "my_record");
        assert_eq!(to_snake_case("Point"), "point");
    }

    #[test]
    fn snake_case_breaks_acronyms_before_following_word() {
        assert_eq!(to_snake_case("HTTPHeader"), "http_header");
        assert_eq!(to_snake_case("XMLParser"), "xml_parser");
        assert_eq!(to_snake_case("MyHTTPClient"), "my_http_client");
    }

    #[test]
    fn snake_case_collapses_pure_acronyms() {
        assert_eq!(to_snake_case("HTTP"), "http");
        assert_eq!(to_snake_case("URL"), "url");
    }

    #[test]
    fn snake_case_passes_through_lowercase() {
        assert_eq!(to_snake_case("point"), "point");
        assert_eq!(to_snake_case("my_record"), "my_record");
    }

    #[test]
    fn snake_case_treats_digit_then_upper_as_word_break() {
        assert_eq!(to_snake_case("Point2D"), "point2_d");
        assert_eq!(to_snake_case("Vector3"), "vector3");
    }

    #[test]
    fn member_symbol_name_uses_owner_and_member() {
        assert_eq!(
            member_symbol_name("MyRecord", "translate"),
            "boltffi_my_record_translate"
        );
    }

    #[test]
    fn canonical_new_symbol_uses_owner_only() {
        assert_eq!(canonical_new_symbol_name("Point"), "boltffi_point_new");
    }

    #[test]
    fn allocator_mints_fresh_ids() {
        let mut allocator = SymbolAllocator::new();
        let first = allocator
            .mint("boltffi_demo_one".to_owned())
            .expect("valid name");
        let second = allocator
            .mint("boltffi_demo_two".to_owned())
            .expect("valid name");
        assert_ne!(first.id(), second.id());
        assert_eq!(first.id().raw(), 0);
        assert_eq!(second.id().raw(), 1);
    }
}
