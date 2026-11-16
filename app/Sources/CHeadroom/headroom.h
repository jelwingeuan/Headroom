// The whole C ABI between the Rust core (libheadroom_core.a) and the Swift shell.
#include <stdint.h>

// Runs the CLI/MCP with the process's arguments (argv[0] included). Returns the exit status.
int32_t headroom_main(int32_t argc, const char *const *argv);

// One request from the UI as JSON, e.g. {"op":"status"}; returns a JSON reply the caller frees.
char *headroom_call(const char *request_json);
void headroom_free(char *reply);
