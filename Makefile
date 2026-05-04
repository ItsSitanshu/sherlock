.PHONY: all gen

all: gen

gen:
	rustc src/synthesizer.rs -o synthesizer
	./synthesizer