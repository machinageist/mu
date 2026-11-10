/* sample.c: text that Markdown would mangle */
#include <stdio.h>
# define TWICE(x) ((x) * 2)

static int _count = 0;

int main(int argc, char **argv) {
	// *not emphasis* and __not bold__
	for (int i = 0; i < argc; i++) {
		printf("%s\n", argv[i]);
	}
    - not a list
    > not a quote
	return TWICE(_count);
}
