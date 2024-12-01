//! [ECMAScript Module Record](https://tc39.es/ecma262/#sec-abstract-module-records)
#![allow(missing_docs)] // fixme

use std::fmt;

use oxc_allocator::{Allocator, Vec};
use oxc_span::{Atom, Span};

use rustc_hash::FxHashMap;

/// ESM Module Record
///
/// All data inside this data structure are for ESM, no commonjs data is allowed.
///
/// See
/// * <https://tc39.es/ecma262/#table-additional-fields-of-source-text-module-records>
/// * <https://tc39.es/ecma262/#cyclic-module-record>
pub struct ModuleRecord<'a> {
    /// This module has no import / export statements
    pub not_esm: bool,

    /// `[[RequestedModules]]`
    ///
    /// A List of all the ModuleSpecifier strings used by the module represented by this record to request the importation of a module. The List is in source text occurrence order.
    ///
    /// Module requests from:
    ///   import ImportClause FromClause
    ///   import ModuleSpecifier
    ///   export ExportFromClause FromClause
    /// Keyed by ModuleSpecifier, valued by all node occurrences
    pub requested_modules: FxHashMap<Atom<'a>, Vec<'a, RequestedModule>>,

    /// `[[ImportEntries]]`
    ///
    /// A List of ImportEntry records derived from the code of this module
    pub import_entries: Vec<'a, ImportEntry<'a>>,

    /// `[[LocalExportEntries]]`
    ///
    /// A List of [`ExportEntry`] records derived from the code of this module
    /// that correspond to declarations that occur within the module
    pub local_export_entries: Vec<'a, ExportEntry<'a>>,

    /// `[[IndirectExportEntries]]`
    ///
    /// A List of [`ExportEntry`] records derived from the code of this module
    /// that correspond to reexported imports that occur within the module
    /// or exports from `export * as namespace` declarations.
    pub indirect_export_entries: Vec<'a, ExportEntry<'a>>,

    /// `[[StarExportEntries]]`
    ///
    /// A List of [`ExportEntry`] records derived from the code of this module
    /// that correspond to `export *` declarations that occur within the module,
    /// not including `export * as namespace` declarations.
    pub star_export_entries: Vec<'a, ExportEntry<'a>>,

    /// Local exported bindings
    pub exported_bindings: FxHashMap<Atom<'a>, Span>,

    /// Local duplicated exported bindings, for diagnostics
    pub exported_bindings_duplicated: Vec<'a, NameSpan<'a>>,

    /// Reexported bindings from `export * from 'specifier'`
    /// Keyed by resolved path
    pub exported_bindings_from_star_export: FxHashMap<Atom<'a>, Vec<'a, Atom<'a>>>,

    /// `export default name`
    ///         ^^^^^^^ span
    pub export_default: Option<Span>,

    /// Duplicated span of `export default` for diagnostics
    pub export_default_duplicated: Vec<'a, Span>,
}

impl<'a> ModuleRecord<'a> {
    pub fn new(allocator: &'a Allocator) -> Self {
        Self {
            not_esm: true,
            requested_modules: FxHashMap::default(),
            import_entries: Vec::new_in(allocator),
            local_export_entries: Vec::new_in(allocator),
            indirect_export_entries: Vec::new_in(allocator),
            star_export_entries: Vec::new_in(allocator),
            exported_bindings: FxHashMap::default(),
            exported_bindings_duplicated: Vec::new_in(allocator),
            exported_bindings_from_star_export: FxHashMap::default(),
            export_default: None,
            export_default_duplicated: Vec::new_in(allocator),
        }
    }
}

impl fmt::Debug for ModuleRecord<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleRecord")
            .field("not_esm", &self.not_esm)
            .field("import_entries", &self.import_entries)
            .field("local_export_entries", &self.local_export_entries)
            .field("indirect_export_entries", &self.indirect_export_entries)
            .field("star_export_entries", &self.star_export_entries)
            .field("exported_bindings", &self.exported_bindings)
            .field("exported_bindings_duplicated", &self.exported_bindings_duplicated)
            .field("exported_bindings_from_star_export", &self.exported_bindings_from_star_export)
            .field("export_default", &self.export_default)
            .field("export_default_duplicated", &self.export_default_duplicated)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameSpan<'a> {
    pub name: Atom<'a>,
    pub span: Span,
}

impl<'a> NameSpan<'a> {
    pub fn new(name: Atom<'a>, span: Span) -> Self {
        Self { name, span }
    }
}

/// [`ImportEntry`](https://tc39.es/ecma262/#importentry-record)
///
/// ## Examples
///
/// ```ts
/// //     _ local_name
/// import v from "mod";
/// //             ^^^ module_request
///
/// //     ____ is_type will be `true`
/// import type { foo as bar } from "mod";
/// // import_name^^^    ^^^ local_name
///
/// import * as ns from "mod";
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEntry<'a> {
    /// String value of the ModuleSpecifier of the ImportDeclaration.
    ///
    /// ## Examples
    ///
    /// ```ts
    /// import { foo } from "mod";
    /// //                   ^^^
    /// ```
    pub module_request: NameSpan<'a>,

    /// The name under which the desired binding is exported by the module identified by `[[ModuleRequest]]`.
    ///
    /// ## Examples
    ///
    /// ```ts
    /// import { foo } from "mod";
    /// //       ^^^
    /// import { foo as bar } from "mod";
    /// //       ^^^
    /// ```
    pub import_name: ImportImportName<'a>,

    /// The name that is used to locally access the imported value from within the importing module.
    ///
    /// ## Examples
    ///
    /// ```ts
    /// import { foo } from "mod";
    /// //       ^^^
    /// import { foo as bar } from "mod";
    /// //              ^^^
    /// ```
    pub local_name: NameSpan<'a>,

    /// Whether this binding is for a TypeScript type-only import. This is a non-standard field.
    /// When creating a [`ModuleRecord`] for a JavaScript file, this will always be false.
    ///
    /// ## Examples
    ///
    /// `is_type` will be `true` for the following imports:
    /// ```ts
    /// import type { foo } from "mod";
    /// import { type foo } from "mod";
    /// ```
    ///
    /// and will be `false` for these imports:
    /// ```ts
    /// import { foo } from "mod";
    /// import { foo as type } from "mod";
    /// ```
    pub is_type: bool,
}

/// `ImportName` For `ImportEntry`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportImportName<'a> {
    Name(NameSpan<'a>),
    NamespaceObject,
    Default(Span),
}

impl ImportImportName<'_> {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Default(_))
    }

    pub fn is_namespace_object(&self) -> bool {
        matches!(self, Self::NamespaceObject)
    }
}

/// [`ExportEntry`](https://tc39.es/ecma262/#exportentry-record)
///
/// Describes a single exported binding from a module. Named export statements that contain more
/// than one binding produce multiple ExportEntry records.
///
/// ## Examples
///
/// ```ts
/// // foo's ExportEntry nas no `module_request` or `import_name.
/// //       ___ local_name
/// export { foo };
/// //       ^^^ export_name. Since there's no alias, it's the same as local_name.
///
/// // re-exports do not produce local bindings, so `local_name` is null.
/// //       ___ import_name    __ module_request
/// export { foo as bar } from "mod";
/// //              ^^^ export_name
///
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExportEntry<'a> {
    /// Span for the entire export entry
    pub span: Span,

    /// The String value of the ModuleSpecifier of the ExportDeclaration.
    /// null if the ExportDeclaration does not have a ModuleSpecifier.
    pub module_request: Option<NameSpan<'a>>,

    /// The name under which the desired binding is exported by the module identified by `[[ModuleRequest]]`.
    /// null if the ExportDeclaration does not have a ModuleSpecifier.
    /// "all" is used for `export * as ns from "mod"`` declarations.
    /// "all-but-default" is used for `export * from "mod" declarations`.
    pub import_name: ExportImportName<'a>,

    /// The name used to export this binding by this module.
    pub export_name: ExportExportName<'a>,

    /// The name that is used to locally access the exported value from within the importing module.
    /// null if the exported value is not locally accessible from within the module.
    pub local_name: ExportLocalName<'a>,
}

/// `ImportName` for `ExportEntry`
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ExportImportName<'a> {
    Name(NameSpan<'a>),
    /// all is used for export * as ns from "mod" declarations.
    All,
    /// all-but-default is used for export * from "mod" declarations.
    AllButDefault,
    /// the ExportDeclaration does not have a ModuleSpecifier
    #[default]
    Null,
}

impl ExportImportName<'_> {
    pub fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }

    pub fn is_all_but_default(&self) -> bool {
        matches!(self, Self::AllButDefault)
    }
}

/// `ExportName` for `ExportEntry`
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ExportExportName<'a> {
    Name(NameSpan<'a>),
    Default(Span),
    #[default]
    Null,
}

impl ExportExportName<'_> {
    /// Returns `true` if this is [`ExportExportName::Default`].
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Default(_))
    }

    /// Returns `true` if this is [`ExportExportName::Null`].
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Attempt to get the [`Span`] of this export name.
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::Name(name) => Some(name.span),
            Self::Default(span) => Some(*span),
            Self::Null => None,
        }
    }
}

/// `LocalName` for `ExportEntry`
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ExportLocalName<'a> {
    Name(NameSpan<'a>),
    /// `export default name_span`
    Default(NameSpan<'a>),
    #[default]
    Null,
}

impl<'a> ExportLocalName<'a> {
    /// `true` if this is a [`ExportLocalName::Default`].
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Default(_))
    }

    /// `true` if this is a [`ExportLocalName::Null`].
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Get the bound name of this export. [`None`] for [`ExportLocalName::Null`].
    pub fn name(&self) -> Option<&Atom<'a>> {
        match self {
            Self::Name(name) | Self::Default(name) => Some(&name.name),
            Self::Null => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RequestedModule {
    span: Span,
    is_type: bool,
    /// is_import is true if the module is requested by an import statement.
    is_import: bool,
}

impl RequestedModule {
    pub fn new(span: Span, is_type: bool, is_import: bool) -> Self {
        Self { span, is_type, is_import }
    }

    pub fn span(&self) -> Span {
        self.span
    }

    /// `true` if a `type` modifier was used in the import statement.
    ///
    /// ## Examples
    /// ```ts
    /// import type { foo } from "foo"; // true, `type` is on module request
    /// import { type bar } from "bar"; // false, `type` is on specifier
    /// import { baz } from "baz";      // false, no `type` modifier
    /// ```
    pub fn is_type(&self) -> bool {
        self.is_type
    }

    /// `true` if the module is requested by an import statement.
    pub fn is_import(&self) -> bool {
        self.is_import
    }
}

#[cfg(test)]
mod test {
    use oxc_span::Span;

    use super::{ExportExportName, ExportLocalName, ImportImportName, NameSpan};

    #[test]
    fn import_import_name() {
        let name = NameSpan::new("name".into(), Span::new(0, 0));
        assert!(!ImportImportName::Name(name.clone()).is_default());
        assert!(!ImportImportName::NamespaceObject.is_default());
        assert!(ImportImportName::Default(Span::new(0, 0)).is_default());

        assert!(!ImportImportName::Name(name.clone()).is_namespace_object());
        assert!(ImportImportName::NamespaceObject.is_namespace_object());
        assert!(!ImportImportName::Default(Span::new(0, 0)).is_namespace_object());
    }

    #[test]
    fn export_import_name() {
        let name = NameSpan::new("name".into(), Span::new(0, 0));
        assert!(!ExportExportName::Name(name.clone()).is_default());
        assert!(ExportExportName::Default(Span::new(0, 0)).is_default());
        assert!(!ExportExportName::Null.is_default());

        assert!(!ExportExportName::Name(name).is_null());
        assert!(!ExportExportName::Default(Span::new(0, 0)).is_null());
        assert!(ExportExportName::Null.is_null());
    }

    #[test]
    fn export_local_name() {
        let name = NameSpan::new("name".into(), Span::new(0, 0));
        assert!(!ExportLocalName::Name(name.clone()).is_default());
        assert!(ExportLocalName::Default(name.clone()).is_default());
        assert!(!ExportLocalName::Null.is_default());

        assert!(!ExportLocalName::Name(name.clone()).is_null());
        assert!(!ExportLocalName::Default(name.clone()).is_null());
        assert!(ExportLocalName::Null.is_null());
    }
}
