use serde::{Deserialize, Serialize};

use crate::{
    CallableDecl, CallbackId, CanonicalName, ClassId, CodecPlan, ConstantId, CustomTypeId,
    DeclMeta, DeclarationId, DefaultValue, ElementMeta, EnumId, FunctionId, HandleRepr,
    InitializerId, IntegerRepr, IntegerValue, MethodId, ReadPlan, RecordId, RecordLayout,
    ReturnTypeRef, StreamId, TypeRef, WritePlan,
};

/// One classified declaration in a binding contract.
///
/// The variants enumerate every kind of FFI-exported item the contract can
/// describe. Each variant carries a fully resolved declaration: matching
/// on the variant and then on the inner shape yields a value with every
/// FFI decision already made.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Decl {
    /// Record declaration.
    Record(Box<RecordDecl>),
    /// Enum declaration.
    Enum(Box<EnumDecl>),
    /// Free function declaration.
    Function(Box<FunctionDecl>),
    /// Class-style object declaration.
    Class(Box<ClassDecl>),
    /// Callback trait declaration.
    Callback(Box<CallbackDecl>),
    /// Stream declaration.
    Stream(Box<StreamDecl>),
    /// Constant declaration.
    Constant(Box<ConstantDecl>),
    /// Custom type declaration.
    CustomType(Box<CustomTypeDecl>),
}

impl Decl {
    /// Returns the typed identity of this declaration.
    pub fn id(&self) -> DeclarationId {
        match self {
            Self::Record(record) => DeclarationId::Record(record.id()),
            Self::Enum(enum_decl) => DeclarationId::Enum(enum_decl.id()),
            Self::Function(function) => DeclarationId::Function(function.id()),
            Self::Class(class) => DeclarationId::Class(class.id()),
            Self::Callback(callback) => DeclarationId::Callback(callback.id()),
            Self::Stream(stream) => DeclarationId::Stream(stream.id()),
            Self::Constant(constant) => DeclarationId::Constant(constant.id()),
            Self::CustomType(custom) => DeclarationId::CustomType(custom.id()),
        }
    }
}

/// A user-defined record after the classifier chose how it crosses.
///
/// `Direct` means the record's bytes themselves are the wire shape and
/// foreign code reads them by offset. `Encoded` means the record is
/// serialized into the contract's wire format and reconstructed on the
/// other side. The two paths carry different fields and different plans;
/// the variant is the choice.
///
/// # Example
///
/// `struct Point { x: f64, y: f64 }` typically classifies as `Direct`
/// because both halves are primitives with predictable layout.
/// `struct UserProfile { name: String, friends: Vec<UserProfile> }`
/// classifies as `Encoded` because its bytes are not a single Rust memory
/// shape.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RecordDecl {
    /// Crosses by raw memory.
    Direct(DirectRecordDecl),
    /// Crosses through encoded bytes.
    Encoded(EncodedRecordDecl),
}

impl RecordDecl {
    /// Returns the record id.
    pub const fn id(&self) -> RecordId {
        match self {
            Self::Direct(record) => record.id(),
            Self::Encoded(record) => record.id(),
        }
    }

    /// Returns the canonical record name.
    pub fn name(&self) -> &CanonicalName {
        match self {
            Self::Direct(record) => record.name(),
            Self::Encoded(record) => record.name(),
        }
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        match self {
            Self::Direct(record) => record.meta(),
            Self::Encoded(record) => record.meta(),
        }
    }
}

/// A record that crosses the boundary as raw memory.
///
/// Carries the byte-level [`RecordLayout`] alongside its fields so foreign
/// code reads each field at the agreed-upon offset rather than asking
/// Rust to serialize on every call.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DirectRecordDecl {
    id: RecordId,
    name: CanonicalName,
    meta: DeclMeta,
    fields: Vec<DirectFieldDecl>,
    initializers: Vec<InitializerDecl>,
    methods: Vec<MethodDecl>,
    layout: RecordLayout,
}

impl DirectRecordDecl {
    pub(crate) fn new(
        id: RecordId,
        name: CanonicalName,
        meta: DeclMeta,
        fields: Vec<DirectFieldDecl>,
        initializers: Vec<InitializerDecl>,
        methods: Vec<MethodDecl>,
        layout: RecordLayout,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            fields,
            initializers,
            methods,
            layout,
        }
    }

    /// Returns the record id.
    pub const fn id(&self) -> RecordId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the fields in source order.
    pub fn fields(&self) -> &[DirectFieldDecl] {
        &self.fields
    }

    /// Returns the initializers.
    pub fn initializers(&self) -> &[InitializerDecl] {
        &self.initializers
    }

    /// Returns the methods.
    pub fn methods(&self) -> &[MethodDecl] {
        &self.methods
    }

    /// Returns the byte-level layout.
    pub fn layout(&self) -> &RecordLayout {
        &self.layout
    }
}

/// A record that crosses the boundary through encoded bytes.
///
/// Each field carries its own per-field codec, and the record itself
/// carries a [`CodecPlan`] for moving the whole value in either direction at
/// once. Initializers and methods are listed beside the value the same way
/// they are on a direct record.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct EncodedRecordDecl {
    id: RecordId,
    name: CanonicalName,
    meta: DeclMeta,
    fields: Vec<EncodedFieldDecl>,
    initializers: Vec<InitializerDecl>,
    methods: Vec<MethodDecl>,
    codec: CodecPlan,
}

impl EncodedRecordDecl {
    pub(crate) fn new(
        id: RecordId,
        name: CanonicalName,
        meta: DeclMeta,
        fields: Vec<EncodedFieldDecl>,
        initializers: Vec<InitializerDecl>,
        methods: Vec<MethodDecl>,
        codec: CodecPlan,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            fields,
            initializers,
            methods,
            codec,
        }
    }

    /// Returns the record id.
    pub const fn id(&self) -> RecordId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the fields in source order.
    pub fn fields(&self) -> &[EncodedFieldDecl] {
        &self.fields
    }

    /// Returns the initializers.
    pub fn initializers(&self) -> &[InitializerDecl] {
        &self.initializers
    }

    /// Returns the methods.
    pub fn methods(&self) -> &[MethodDecl] {
        &self.methods
    }

    /// Returns the whole-record read plan.
    pub fn read(&self) -> &ReadPlan {
        self.codec.read()
    }

    /// Returns the whole-record write plan.
    pub fn write(&self) -> &WritePlan {
        self.codec.write()
    }

    /// Returns the whole-record codec.
    pub fn codec(&self) -> &CodecPlan {
        &self.codec
    }
}

/// How a field is named inside a record or variant payload.
///
/// `Named` is a regular struct field; `Position` is a tuple-style field
/// addressed by its zero-based index. Mixing the two inside one record or
/// one variant payload is rejected at validation.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FieldKey {
    /// Named field.
    Named(CanonicalName),
    /// Tuple field at the given zero-based position.
    Position(u32),
}

/// One field of a direct record.
///
/// Field offsets live on the parent record's [`RecordLayout`] rather than
/// on the field itself, so the layout can be validated as one coherent
/// value before any consumer reads from it.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DirectFieldDecl {
    key: FieldKey,
    ty: TypeRef,
    meta: ElementMeta,
}

impl DirectFieldDecl {
    pub(crate) fn new(key: FieldKey, ty: TypeRef, meta: ElementMeta) -> Self {
        Self { key, ty, meta }
    }

    /// Returns the field key.
    pub fn key(&self) -> &FieldKey {
        &self.key
    }

    /// Returns the field type.
    pub fn ty(&self) -> &TypeRef {
        &self.ty
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }
}

/// One field of an encoded record or data enum payload.
///
/// Carries its own [`CodecPlan`] because each field is
/// encoded independently inside the parent value's wire format.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct EncodedFieldDecl {
    key: FieldKey,
    ty: TypeRef,
    codec: CodecPlan,
    meta: ElementMeta,
}

impl EncodedFieldDecl {
    pub(crate) fn new(key: FieldKey, ty: TypeRef, codec: CodecPlan, meta: ElementMeta) -> Self {
        Self {
            key,
            ty,
            codec,
            meta,
        }
    }

    /// Returns the field key.
    pub fn key(&self) -> &FieldKey {
        &self.key
    }

    /// Returns the field type.
    pub fn ty(&self) -> &TypeRef {
        &self.ty
    }

    /// Returns the read plan.
    pub fn read(&self) -> &ReadPlan {
        self.codec.read()
    }

    /// Returns the write plan.
    pub fn write(&self) -> &WritePlan {
        self.codec.write()
    }

    /// Returns the field codec.
    pub fn codec(&self) -> &CodecPlan {
        &self.codec
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }
}

/// A user-defined enum after the classifier chose how it crosses.
///
/// `CStyle` is a fieldless enum represented by a primitive integer
/// discriminant; both sides agree on which integer means which variant.
/// `Data` is a payload-carrying enum represented by an encoded tag plus
/// per-variant payload bytes.
///
/// # Example
///
/// `enum Direction { North, South, East, West }` classifies as `CStyle`.
/// `enum Event { Click(Point), KeyDown(String) }` classifies as `Data`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EnumDecl {
    /// Fieldless enum represented by an integer discriminant.
    CStyle(CStyleEnumDecl),
    /// Payload-carrying enum represented by an encoded tag and payload.
    Data(Box<DataEnumDecl>),
}

impl EnumDecl {
    /// Returns the enum id.
    pub const fn id(&self) -> EnumId {
        match self {
            Self::CStyle(enum_decl) => enum_decl.id(),
            Self::Data(enum_decl) => enum_decl.id(),
        }
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        match self {
            Self::CStyle(enum_decl) => enum_decl.name(),
            Self::Data(enum_decl) => enum_decl.name(),
        }
    }
}

/// A fieldless enum whose variants are integer values.
///
/// The integer representation determines how the discriminant crosses the
/// boundary. Variants list their resolved discriminant values; two
/// variants with the same discriminant are rejected at validation.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CStyleEnumDecl {
    id: EnumId,
    name: CanonicalName,
    meta: DeclMeta,
    repr: IntegerRepr,
    variants: Vec<CStyleVariantDecl>,
    methods: Vec<MethodDecl>,
}

impl CStyleEnumDecl {
    pub(crate) fn new(
        id: EnumId,
        name: CanonicalName,
        meta: DeclMeta,
        repr: IntegerRepr,
        variants: Vec<CStyleVariantDecl>,
        methods: Vec<MethodDecl>,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            repr,
            variants,
            methods,
        }
    }

    /// Returns the enum id.
    pub const fn id(&self) -> EnumId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the discriminant representation.
    pub const fn repr(&self) -> IntegerRepr {
        self.repr
    }

    /// Returns the variants in source order.
    pub fn variants(&self) -> &[CStyleVariantDecl] {
        &self.variants
    }

    /// Returns the methods.
    pub fn methods(&self) -> &[MethodDecl] {
        &self.methods
    }
}

/// One variant of a fieldless enum.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CStyleVariantDecl {
    name: CanonicalName,
    discriminant: IntegerValue,
    meta: ElementMeta,
}

impl CStyleVariantDecl {
    pub(crate) fn new(name: CanonicalName, discriminant: IntegerValue, meta: ElementMeta) -> Self {
        Self {
            name,
            discriminant,
            meta,
        }
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the discriminant value.
    pub const fn discriminant(&self) -> IntegerValue {
        self.discriminant
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }
}

/// An enum whose variants can carry data.
///
/// Crosses the boundary as a tag followed by an encoded payload. Each
/// variant carries its own payload shape; the codec plan
/// describe the dispatch over the tag.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DataEnumDecl {
    id: EnumId,
    name: CanonicalName,
    meta: DeclMeta,
    variants: Vec<DataVariantDecl>,
    methods: Vec<MethodDecl>,
    codec: CodecPlan,
}

impl DataEnumDecl {
    pub(crate) fn new(
        id: EnumId,
        name: CanonicalName,
        meta: DeclMeta,
        variants: Vec<DataVariantDecl>,
        methods: Vec<MethodDecl>,
        codec: CodecPlan,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            variants,
            methods,
            codec,
        }
    }

    /// Returns the enum id.
    pub const fn id(&self) -> EnumId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the variants in source order.
    pub fn variants(&self) -> &[DataVariantDecl] {
        &self.variants
    }

    /// Returns the methods.
    pub fn methods(&self) -> &[MethodDecl] {
        &self.methods
    }

    /// Returns the whole-enum read plan.
    pub fn read(&self) -> &ReadPlan {
        self.codec.read()
    }

    /// Returns the whole-enum write plan.
    pub fn write(&self) -> &WritePlan {
        self.codec.write()
    }

    /// Returns the whole-enum codec.
    pub fn codec(&self) -> &CodecPlan {
        &self.codec
    }
}

/// The integer tag assigned to one data enum variant.
///
/// Stable for the life of a contract. Two variants in the same enum
/// cannot share a tag.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VariantTag(u32);

impl VariantTag {
    pub(crate) fn new(tag: u32) -> Self {
        Self(tag)
    }

    /// Returns the tag value.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// One variant of a data enum.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DataVariantDecl {
    name: CanonicalName,
    tag: VariantTag,
    payload: DataVariantPayload,
    meta: ElementMeta,
}

impl DataVariantDecl {
    pub(crate) fn new(
        name: CanonicalName,
        tag: VariantTag,
        payload: DataVariantPayload,
        meta: ElementMeta,
    ) -> Self {
        Self {
            name,
            tag,
            payload,
            meta,
        }
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the tag.
    pub const fn tag(&self) -> VariantTag {
        self.tag
    }

    /// Returns the payload shape.
    pub fn payload(&self) -> &DataVariantPayload {
        &self.payload
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }
}

/// The data carried by one variant of a data enum.
///
/// `Unit` is no payload; `Tuple` is positional fields like `Click(Point)`;
/// `Struct` is named fields like `Move { x, y }`. The shape determines
/// how foreign generated code spells the variant constructor and pattern.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DataVariantPayload {
    /// Variant without payload.
    Unit,
    /// Tuple-style payload fields.
    Tuple(Vec<EncodedFieldDecl>),
    /// Struct-style payload fields.
    Struct(Vec<EncodedFieldDecl>),
}

/// A free function exported across the boundary.
///
/// Carries the binding name and the [`CallableDecl`] that describes how
/// the call actually crosses. The native symbol lives on the inner
/// callable.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct FunctionDecl {
    id: FunctionId,
    name: CanonicalName,
    meta: DeclMeta,
    callable: CallableDecl,
}

impl FunctionDecl {
    pub(crate) fn new(
        id: FunctionId,
        name: CanonicalName,
        meta: DeclMeta,
        callable: CallableDecl,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            callable,
        }
    }

    /// Returns the function id.
    pub const fn id(&self) -> FunctionId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the callable.
    pub fn callable(&self) -> &CallableDecl {
        &self.callable
    }
}

/// A Rust type exposed as a class-style object.
///
/// Foreign code holds a handle that names a Rust-owned instance.
/// Initializers construct new instances and methods cross as ordinary
/// callables that take the handle as their receiver. The handle
/// representation determines whether the carrier is a 64-bit integer or a
/// pointer-width value.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ClassDecl {
    id: ClassId,
    name: CanonicalName,
    meta: DeclMeta,
    handle: HandleRepr,
    initializers: Vec<InitializerDecl>,
    methods: Vec<MethodDecl>,
}

impl ClassDecl {
    pub(crate) fn new(
        id: ClassId,
        name: CanonicalName,
        meta: DeclMeta,
        handle: HandleRepr,
        initializers: Vec<InitializerDecl>,
        methods: Vec<MethodDecl>,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            handle,
            initializers,
            methods,
        }
    }

    /// Returns the class id.
    pub const fn id(&self) -> ClassId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the handle carrier.
    pub const fn handle(&self) -> HandleRepr {
        self.handle
    }

    /// Returns the initializers.
    pub fn initializers(&self) -> &[InitializerDecl] {
        &self.initializers
    }

    /// Returns the methods.
    pub fn methods(&self) -> &[MethodDecl] {
        &self.methods
    }
}

/// A foreign-implemented trait whose methods Rust can call.
///
/// Foreign code provides an object that implements the trait; Rust holds
/// a handle to that object and invokes its methods through the callback
/// dispatch table. Each trait method is a [`MethodDecl`].
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CallbackDecl {
    id: CallbackId,
    name: CanonicalName,
    meta: DeclMeta,
    handle: HandleRepr,
    methods: Vec<MethodDecl>,
}

impl CallbackDecl {
    pub(crate) fn new(
        id: CallbackId,
        name: CanonicalName,
        meta: DeclMeta,
        handle: HandleRepr,
        methods: Vec<MethodDecl>,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            handle,
            methods,
        }
    }

    /// Returns the callback id.
    pub const fn id(&self) -> CallbackId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the handle carrier.
    pub const fn handle(&self) -> HandleRepr {
        self.handle
    }

    /// Returns the methods the foreign implementation must provide.
    pub fn methods(&self) -> &[MethodDecl] {
        &self.methods
    }
}

/// An asynchronous sequence of values produced by Rust.
///
/// Foreign code holds a handle and pulls items as they arrive. The item
/// type determines how each value is encoded on its way out.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct StreamDecl {
    id: StreamId,
    name: CanonicalName,
    meta: DeclMeta,
    handle: HandleRepr,
    item: TypeRef,
}

impl StreamDecl {
    pub(crate) fn new(
        id: StreamId,
        name: CanonicalName,
        meta: DeclMeta,
        handle: HandleRepr,
        item: TypeRef,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            handle,
            item,
        }
    }

    /// Returns the stream id.
    pub const fn id(&self) -> StreamId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the handle carrier.
    pub const fn handle(&self) -> HandleRepr {
        self.handle
    }

    /// Returns the item type.
    pub fn item(&self) -> &TypeRef {
        &self.item
    }
}

/// A named constant value the contract exposes.
///
/// May either be inlined (the literal is part of the binding) or accessed
/// through a callable (the value is computed at runtime).
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ConstantDecl {
    id: ConstantId,
    name: CanonicalName,
    meta: DeclMeta,
    value: ConstantValueDecl,
}

impl ConstantDecl {
    pub(crate) fn new(
        id: ConstantId,
        name: CanonicalName,
        meta: DeclMeta,
        value: ConstantValueDecl,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            value,
        }
    }

    /// Returns the constant id.
    pub const fn id(&self) -> ConstantId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the value shape.
    pub fn value(&self) -> &ConstantValueDecl {
        &self.value
    }
}

/// How a constant's value is delivered to foreign code.
///
/// `Inline` lets generated bindings emit the literal directly with no FFI
/// call. `Accessor` is used when the value cannot be expressed as a
/// literal: foreign code calls the accessor to read it.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ConstantValueDecl {
    /// Emit the literal value directly in generated source.
    Inline {
        /// Type of the inline literal.
        ty: TypeRef,
        /// The literal value.
        value: DefaultValue,
    },
    /// Read the value through a native accessor.
    Accessor(Box<CallableDecl>),
}

/// A user-defined type that maps to an existing binding shape.
///
/// Custom types let users layer a Rust newtype on top of a standard
/// binding type without introducing a new representation. The value
/// crosses the boundary as the underlying [`TypeRef`].
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CustomTypeDecl {
    id: CustomTypeId,
    name: CanonicalName,
    meta: DeclMeta,
    representation: TypeRef,
}

impl CustomTypeDecl {
    pub(crate) fn new(
        id: CustomTypeId,
        name: CanonicalName,
        meta: DeclMeta,
        representation: TypeRef,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            representation,
        }
    }

    /// Returns the custom type id.
    pub const fn id(&self) -> CustomTypeId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the underlying representation.
    pub fn representation(&self) -> &TypeRef {
        &self.representation
    }
}

/// A method on a record, enum, class, or callback trait.
///
/// Owned by its parent declaration. The id is unique inside the owner;
/// two records can each have a method with the same id without conflict.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct MethodDecl {
    id: MethodId,
    name: CanonicalName,
    meta: DeclMeta,
    callable: CallableDecl,
}

impl MethodDecl {
    pub(crate) fn new(
        id: MethodId,
        name: CanonicalName,
        meta: DeclMeta,
        callable: CallableDecl,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            callable,
        }
    }

    /// Returns the method id.
    pub const fn id(&self) -> MethodId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the callable.
    pub fn callable(&self) -> &CallableDecl {
        &self.callable
    }
}

/// A callable selected to be exposed as a target language constructor.
///
/// Rust does not have constructors; it has associated functions that
/// happen to return `Self`. The classifier picks a subset of those and
/// promotes them to initializers so target languages can spell them as
/// `Point.init(x:y:)`, `Point(x = ..., y = ...)`, or whatever the host
/// idiom is.
///
/// # Example
///
/// `impl Point { pub fn new(x: f64, y: f64) -> Self }` classifies as an
/// initializer. `impl Point { pub fn distance(&self) -> f64 }` does not,
/// because it is not a constructor.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct InitializerDecl {
    id: InitializerId,
    name: CanonicalName,
    meta: DeclMeta,
    callable: CallableDecl,
    returns: ReturnTypeRef,
}

impl InitializerDecl {
    pub(crate) fn new(
        id: InitializerId,
        name: CanonicalName,
        meta: DeclMeta,
        callable: CallableDecl,
        returns: ReturnTypeRef,
    ) -> Self {
        Self {
            id,
            name,
            meta,
            callable,
            returns,
        }
    }

    /// Returns the initializer id.
    pub const fn id(&self) -> InitializerId {
        self.id
    }

    /// Returns the canonical name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the declaration metadata.
    pub fn meta(&self) -> &DeclMeta {
        &self.meta
    }

    /// Returns the callable.
    pub fn callable(&self) -> &CallableDecl {
        &self.callable
    }

    /// Returns the constructed type.
    pub fn returns(&self) -> &ReturnTypeRef {
        &self.returns
    }
}
