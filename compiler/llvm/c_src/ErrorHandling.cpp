#include "llvm/Support/ErrorHandling.h"
#include "llvm/Support/Signals.h"

#include <cstddef>
#include <cstdio>
#include <cstdlib>

using namespace llvm;

static void FireflyFatalErrorHandler(void *, const char *reason, bool) {
  fprintf(stderr, "LLVM FATAL ERROR: %s\n", reason);
  ::abort();
}

extern "C" void LLVMFireflyInstallFatalErrorHandler(void) {
  llvm::remove_fatal_error_handler();
  llvm::install_fatal_error_handler(FireflyFatalErrorHandler);
}
