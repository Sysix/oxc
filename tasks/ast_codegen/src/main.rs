use std::{cell::RefCell, collections::HashMap, io::Read, path::PathBuf, rc::Rc};

use bpaf::{Bpaf, Parser};
use itertools::Itertools;
use proc_macro2::TokenStream;
use syn::parse_file;

mod fmt;
mod generators;
mod layout;
mod markers;
mod passes;
mod rust_ast;
mod schema;
mod util;

use fmt::{cargo_fmt, pprint};
use generators::{
    AssertLayouts, AstBuilderGenerator, AstKindGenerator, Generator, VisitGenerator,
    VisitMutGenerator,
};
use passes::{CalcLayout, Linker, Pass};
use rust_ast::AstRef;
use schema::{lower_ast_types, Schema, TypeDef};
use util::{write_all_to, NormalizeError};

use crate::generators::ImplGetSpanGenerator;

static SOURCE_PATHS: &[&str] = &[
    "oxc_ast/src/ast/literal.rs",
    "oxc_ast/src/ast/js.rs",
    "oxc_ast/src/ast/ts.rs",
    "oxc_ast/src/ast/jsx.rs",
    "oxc_syntax/src/number.rs",
    "oxc_syntax/src/operator.rs",
    "oxc_span/src/span/types.rs",
    "oxc_span/src/source_type/types.rs",
];

const AST_CRATE: &str = "crates/oxc_ast";
#[allow(dead_code)]
const AST_MACROS_CRATE: &str = "crates/oxc_ast_macros";

type Result<R> = std::result::Result<R, String>;
type TypeId = usize;
type TypeTable = Vec<AstRef>;
type DefTable = Vec<TypeDef>;
type IdentTable = HashMap<String, TypeId>;

#[derive(Default)]
struct AstCodegen {
    files: Vec<PathBuf>,
    passes: Vec<Box<dyn Runner<Output = (), Context = EarlyCtx>>>,
    builders: Vec<Box<dyn Runner<Output = GeneratorOutput, Context = LateCtx>>>,
}

type GeneratedStream = (/* output path */ PathBuf, TokenStream);
type DataStream = (/* output path */ PathBuf, Vec<u8>);

// TODO: remove me
#[allow(dead_code)]
#[derive(Debug, Clone)]
enum GeneratorOutput {
    None,
    Info(Vec<u8>),
    Data(DataStream),
    Stream(GeneratedStream),
}

// TODO: remove me
#[allow(dead_code)]
impl GeneratorOutput {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    pub fn expect_none(&self) {
        assert!(self.is_none());
    }

    pub fn to_info(&self) -> &[u8] {
        if let Self::Info(it) = self {
            it
        } else {
            panic!();
        }
    }

    pub fn to_data(&self) -> &DataStream {
        if let Self::Data(it) = self {
            it
        } else {
            panic!();
        }
    }

    pub fn to_stream(&self) -> &GeneratedStream {
        if let Self::Stream(it) = self {
            it
        } else {
            panic!();
        }
    }

    pub fn into_info(self) -> Vec<u8> {
        if let Self::Info(it) = self {
            it
        } else {
            panic!();
        }
    }

    pub fn into_data(self) -> DataStream {
        if let Self::Data(it) = self {
            it
        } else {
            panic!();
        }
    }

    pub fn into_stream(self) -> GeneratedStream {
        if let Self::Stream(it) = self {
            it
        } else {
            panic!();
        }
    }
}

struct EarlyCtx {
    ty_table: TypeTable,
    ident_table: IdentTable,
    mods: RefCell<Vec<rust_ast::Module>>,
}

impl EarlyCtx {
    fn new(mods: Vec<rust_ast::Module>) -> Self {
        // worst case len
        let len = mods.iter().fold(0, |acc, it| acc + it.items.len());
        let adts = mods.iter().flat_map(|it| it.items.iter());

        let mut ty_table = TypeTable::with_capacity(len);
        let mut ident_table = IdentTable::with_capacity(len);
        for adt in adts {
            if let Some(ident) = adt.borrow().ident() {
                let ident = ident.to_string();
                let type_id = ty_table.len();
                ty_table.push(AstRef::clone(adt));
                ident_table.insert(ident, type_id);
            }
        }

        Self { ty_table, ident_table, mods: RefCell::new(mods) }
    }

    fn into_late_ctx(self) -> LateCtx {
        let schema = lower_ast_types(&self);

        LateCtx { schema }
    }

    fn find(&self, key: &String) -> Option<AstRef> {
        self.type_id(key).map(|id| AstRef::clone(&self.ty_table[id]))
    }

    fn type_id(&self, key: &String) -> Option<TypeId> {
        self.ident_table.get(key).copied()
    }

    fn ast_ref(&self, id: TypeId) -> AstRef {
        AstRef::clone(&self.ty_table[id])
    }
}

struct LateCtx {
    schema: Schema,
}

struct CodegenResult {
    schema: Schema,
    outputs: Vec<(/* generator name */ &'static str, /* output */ GeneratorOutput)>,
}

impl LateCtx {
    fn type_def(&self, id: TypeId) -> Option<&TypeDef> {
        self.schema.definitions.get(id)
    }
}

trait Runner {
    type Context;
    type Output;
    fn name(&self) -> &'static str;
    fn run(&mut self, ctx: &Self::Context) -> Result<Self::Output>;
}

impl AstCodegen {
    #[must_use]
    fn add_file<P>(mut self, path: P) -> Self
    where
        P: AsRef<str>,
    {
        self.files.push(path.as_ref().into());
        self
    }

    #[must_use]
    fn pass<P>(mut self, pass: P) -> Self
    where
        P: Pass + Runner<Output = (), Context = EarlyCtx> + 'static,
    {
        self.passes.push(Box::new(pass));
        self
    }

    #[must_use]
    fn gen<G>(mut self, generator: G) -> Self
    where
        G: Generator + Runner<Output = GeneratorOutput, Context = LateCtx> + 'static,
    {
        self.builders.push(Box::new(generator));
        self
    }

    fn generate(self) -> Result<CodegenResult> {
        let modules = self
            .files
            .into_iter()
            .map(rust_ast::Module::from)
            .map(rust_ast::Module::load)
            .map_ok(rust_ast::Module::expand)
            .map_ok(|it| it.map(rust_ast::Module::analyze))
            .collect::<Result<Result<Result<Vec<_>>>>>()???;

        // early passes
        let ctx = {
            let ctx = EarlyCtx::new(modules);
            _ = self
                .passes
                .into_iter()
                .map(|mut runner| runner.run(&ctx).map(|res| (runner.name(), res)))
                .collect::<Result<Vec<_>>>()?;
            ctx.into_late_ctx()
        };

        let outputs = self
            .builders
            .into_iter()
            .map(|mut runner| runner.run(&ctx).map(|res| (runner.name(), res)))
            .collect::<Result<Vec<_>>>()?;

        Ok(CodegenResult { outputs, schema: ctx.schema })
    }
}

fn files() -> impl std::iter::Iterator<Item = String> {
    SOURCE_PATHS.iter().map(|path| format!("crates/{path}"))
}

fn write_generated_streams(
    streams: impl IntoIterator<Item = GeneratedStream>,
) -> std::io::Result<()> {
    for (path, stream) in streams {
        let content = pprint(&stream);
        write_all_to(content.as_bytes(), path.into_os_string().to_str().unwrap())?;
    }
    Ok(())
}

fn write_data_streams(streams: impl IntoIterator<Item = DataStream>) -> std::io::Result<()> {
    for (path, content) in streams {
        write_all_to(&content, path.into_os_string().to_str().unwrap())?;
    }
    Ok(())
}

#[derive(Debug, Bpaf)]
pub struct CliOptions {
    /// Runs all generators but won't write anything down.
    #[bpaf(switch)]
    dry_run: bool,
    /// Don't run cargo fmt at the end
    #[bpaf(switch)]
    no_fmt: bool,
    /// Path of output `schema.json`.
    schema: Option<std::path::PathBuf>,
}

#[allow(clippy::print_stdout)]
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli_options = cli_options().run();

    let CodegenResult { outputs, schema } = files()
        .fold(AstCodegen::default(), AstCodegen::add_file)
        .pass(Linker)
        .pass(CalcLayout)
        .gen(AssertLayouts)
        .gen(AstKindGenerator)
        .gen(AstBuilderGenerator)
        .gen(ImplGetSpanGenerator)
        .gen(VisitGenerator)
        .gen(VisitMutGenerator)
        .generate()?;

    let (streams, outputs): (Vec<_>, Vec<_>) =
        outputs.into_iter().partition(|it| matches!(it.1, GeneratorOutput::Stream(_)));

    let (binaries, _): (Vec<_>, Vec<_>) =
        outputs.into_iter().partition(|it| matches!(it.1, GeneratorOutput::Data(_)));

    if !cli_options.dry_run {
        write_generated_streams(streams.into_iter().map(|it| it.1.into_stream()))?;
        write_data_streams(binaries.into_iter().map(|it| it.1.into_data()))?;
    }

    if !cli_options.no_fmt {
        cargo_fmt();
    }

    if let CliOptions { schema: Some(schema_path), dry_run: false, .. } = cli_options {
        let path = schema_path.to_str().expect("invalid path for schema output.");
        let schema = serde_json::to_string_pretty(&schema).normalize()?;
        write_all_to(schema.as_bytes(), path)?;
    }

    Ok(())
}

fn output(krate: &str, path: &str) -> PathBuf {
    std::path::PathBuf::from_iter(vec![krate, "src", "generated", path])
}
