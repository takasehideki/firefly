use firefly_rt::function::ErlangResult;
use firefly_rt::term::*;

#[export_name = "file:native_name_encoding/0"]
#[allow(improper_ctypes_definitions)]
pub extern "C-unwind" fn native_name_encoding() -> ErlangResult {
    ErlangResult::Ok(atoms::Utf8.into())
}
