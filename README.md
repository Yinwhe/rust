## Note
This rapo is modified for research purpose: it can **extract and analysis** unstable features used by the code.

## How to build
For *nix system:
```Shell
# Build Setup (Choose "compiler" when meeting setup option)
./x.py setup
# Compile (When met "error: failed to download llvm from ci", follow the instructions given)
./x.py build
```

## How to use
```
rustc --ruf-analysis <file>
```