mod attributes;
mod functions;
mod inject;
mod records;
mod verify;

use liblumen_diagnostics::*;
use liblumen_intern::Ident;
use liblumen_pass::Pass;
use liblumen_syntax_base::ApplicationMetadata;

use crate::ast;

use self::inject::AddAutoImports;
use self::verify::{VerifyCalls, VerifyNifs, VerifyOnLoadFunctions, VerifyTypeSpecs};

pub use self::attributes::analyze_attribute;
pub use self::functions::analyze_function;
pub use self::records::analyze_record;

/// This pass is responsible for taking a set of top-level forms and
/// analyzing them in the context of a new module to produce a fully
/// constructed and initially validated module.
///
/// Some of the analyses run include:
///
/// * If configured to do so, warns if functions are missing type specs
/// * Warns about type specs for undefined functions
/// * Warns about redefined attributes
/// * Errors on invalid nif declarations
/// * Errors on invalid syntax in built-in attributes (e.g. -import(..))
/// * Errors on mismatched function clauses (name/arity)
/// * Errors on unterminated function clauses
/// * Errors on redefined functions
///
/// And a few other similar lints
pub struct SemanticAnalysis<'app> {
    reporter: Reporter,
    app: &'app ApplicationMetadata,
}
impl<'app> SemanticAnalysis<'app> {
    pub fn new(reporter: Reporter, app: &'app ApplicationMetadata) -> Self {
        Self { reporter, app }
    }
}
impl<'app> Pass for SemanticAnalysis<'app> {
    type Input<'a> = ast::Module;
    type Output<'a> = ast::Module;

    fn run<'a>(&mut self, mut module: Self::Input<'a>) -> anyhow::Result<Self::Output<'a>> {
        let mut passes = AddAutoImports
            //.chain(DefinePseudoLocals)
            .chain(VerifyOnLoadFunctions::new(self.reporter.clone()))
            .chain(VerifyTypeSpecs::new(self.reporter.clone()))
            .chain(VerifyNifs::new(self.reporter.clone()))
            .chain(VerifyCalls::new(self.reporter.clone(), self.app));

        passes.run(&mut module)?;

        Ok(module)
    }
}
