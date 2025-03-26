#include <stdio.h>

void foo() {
  printf("Hello, World\n");
}

void bar() {
  foo();
}

void baz() {
  bar();
}

int main() {
  baz();
  return 0;
}
