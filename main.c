#include <stdio.h>

int i=1;

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
