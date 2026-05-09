//! What is in `Bindings`, and how a consumer reads it.
//!
//! When `#[data]` and `#[export]` see the user's source, they classify
//! every exported item: this record is bytes that can cross by memcpy,
//! that enum has a payload that needs encoding, this async function
//! returns a poll handle. By the time a [`Bindings`] reaches a consumer,
//! the decisions are over. Every declaration carries its boundary plan
//! attached.
//!
//! Generating Swift, Kotlin, Python, or any other target language is not
//! in this module. The work here ends at the resolved facts.
//!
//! # The shape of a contract
//!
//! For the source
//!
//! ```ignore
//! use boltffi::*;
//!
//! #[data]
//! pub struct Point { pub x: f64, pub y: f64 }
//!
//! #[export]
//! pub fn distance(a: Point, b: Point) -> f64 { /* ... */ }
//! ```
//!
//! `Point` becomes a [`RecordDecl::Direct`]. Both fields are primitives
//! with predictable layout, so the classifier picks the direct path: 16
//! bytes total, 8-byte alignment, `x` at offset 0, `y` at offset 8.
//! Foreign code reads the struct by offset. With a `String` field, the
//! same source would have produced a [`RecordDecl::Encoded`] instead,
//! carrying a [`ReadPlan`] and a [`WritePlan`] for moving the bytes.
//!
//! `distance` becomes a [`FunctionDecl`]. Inside it, a [`CallableDecl`]
//! holds the native symbol foreign code will call, two [`ParamDecl`]s
//! that lower as direct `Point` values, and a primitive `f64` return.
//! Synchronous. No error path.
//!
//! Both refer back to a [`NativeSymbolTable`] hanging off the `Bindings`
//! value, alongside a [`PackageInfo`] used in diagnostics.
//!
//! # Consuming a contract
//!
//! Pattern match on [`Decl`]:
//!
//! ```ignore
//! for decl in bindings.decls() {
//!     match decl {
//!         Decl::Record(record) => render_record(record),
//!         Decl::Function(function) => render_function(function),
//!         _ => continue,
//!     }
//! }
//! ```
//!
//! Validation runs before the value reaches a consumer. Inside a match
//! arm, every id is unique inside its family, every native symbol is
//! callable, and the schema version is one this code understands. No
//! fallible accessor exists; a held [`Bindings`] is consistent, or
//! construction would have failed.
//!
//! [`Decl`] is the front door. [`RecordDecl`], [`EnumDecl`],
//! [`CallableDecl`], and [`CodecNode`] are where most of the real shape
//! lives.

#![allow(dead_code)]

mod callable;
mod codec;
mod contract;
mod decl;
mod error;
mod ids;
mod layout;
mod metadata;
mod name;
mod op;
mod primitive;
mod symbol;
mod types;

pub use callable::{
    AsyncDecl, CallableDecl, ErrorDecl, ExecutionDecl, LiftPlan, LowerPlan, ParamDecl,
    ReceiverDecl, ReturnDecl,
};
pub use codec::{CodecNode, CodecPlan, ReadPlan, WritePlan};
pub use contract::{Bindings, ContractVersion, PackageInfo};
pub use decl::{
    CStyleEnumDecl, CStyleVariantDecl, CallbackDecl, ClassDecl, ConstantDecl, ConstantValueDecl,
    CustomTypeDecl, DataEnumDecl, DataVariantDecl, DataVariantPayload, Decl, DirectFieldDecl,
    DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl, EnumDecl, FieldKey, FunctionDecl,
    InitializerDecl, MethodDecl, RecordDecl, StreamDecl, VariantTag,
};
pub use error::{BindingError, BindingErrorKind};
pub use ids::{
    CallbackId, ClassId, ConstantId, CustomTypeId, DeclarationId, EnumId, FunctionId,
    InitializerId, MethodId, RecordId, StreamId, SymbolId,
};
pub use layout::{AlignmentError, ByteAlignment, ByteOffset, ByteSize, FieldLayout, RecordLayout};
pub use metadata::{
    DeclMeta, DefaultValue, DeprecationInfo, DocComment, ElementMeta, FloatValue, IntegerValue,
};
pub use name::{CanonicalName, NamePart};
pub use op::{
    BinderId, ByteCount, ElementCount, IntrinsicOp, Op, OpNode, Scalar, ScalarTy, Truth, ValueRef,
    ValueRoot,
};
pub use primitive::{HandleRepr, IntegerRepr, Primitive};
pub use symbol::{NativeSymbol, NativeSymbolTable, SymbolName};
pub use types::{ClosureTypeRef, ReturnTypeRef, TypeRef};
