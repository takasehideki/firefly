use core::fmt;
use core::str::FromStr;

use crate::term::Atom;

use super::FunctionSymbol;

/// This struct is a subset of `FunctionSymbol` that is used to more
/// generally represent module/function/arity information for any function
/// whether defined or not.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleFunctionArity {
    pub module: Atom,
    pub function: Atom,
    pub arity: u8,
}
impl From<FunctionSymbol> for ModuleFunctionArity {
    #[inline]
    fn from(sym: FunctionSymbol) -> Self {
        Self {
            module: sym.module,
            function: sym.function,
            arity: sym.arity,
        }
    }
}
impl FromStr for ModuleFunctionArity {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((module, rest)) = s.split_once(':') else { return Err(()); };
        let Some((function, arity)) = rest.split_once('/') else { return Err(()); };

        let module = Atom::from(module);
        let function = Atom::from(function);
        let Ok(arity) = arity.parse::<u8>() else { return Err(()); };

        Self {
            module,
            function,
            arity,
        }
    }
}
impl fmt::Debug for ModuleFunctionArity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}
impl fmt::Display for ModuleFunctionArity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}/{}", self.module, self.function, self.arity)
    }
}
