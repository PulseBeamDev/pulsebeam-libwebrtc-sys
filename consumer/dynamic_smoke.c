#include <dlfcn.h>
#include <stdio.h>

int main(int argc, char** argv) {
  if (argc != 2) {
    fprintf(stderr, "usage: dynamic-smoke <library>\n");
    return 1;
  }
  void* library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
  if (!library) {
    fprintf(stderr, "dlopen: %s\n", dlerror());
    return 1;
  }
  int (*smoke)(void) = (int (*)(void))dlsym(library, "pulsebeam_dynamic_smoke");
  if (!smoke) {
    fprintf(stderr, "dlsym: %s\n", dlerror());
    dlclose(library);
    return 1;
  }
  int result = smoke();
  if (dlclose(library) != 0) {
    fprintf(stderr, "dlclose: %s\n", dlerror());
    return 1;
  }
  return result;
}
