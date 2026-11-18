# @aphrody/ay-core index and native bindings (C header, Python, Java) (imported by the root justfile).

# Index src (+ third_party/vendor with --all) via @aphrody/ay-core into var/index/core.db
[group('core')]
index *args:
    bun packages/core/src/cli.ts index "{{root}}" {{args}}

# Generate skills + docs/reference from the index
[group('core')]
index-gen:
    bun packages/core/src/cli.ts gen "{{root}}"

# Regenerate the core C header (cbindgen)
[group('core')]
core-header:
    cbindgen --config crates/ay-core/capi/cbindgen.toml crates/ay-core/capi -o crates/ay-core/capi/include/ay_core.h
# --- Labs (merged into src): conformance (ay conformance), Lore, Chromium factory ---

# Build the native Python core (`crates/ay-core/capi/py`, PyO3 ABI3) into target/py/aphrody_native.{pyd,so}
[group('core')]
py-build:
    cargo build -p ay-core-py --release
    mkdir -p "{{root}}/target/py"
    cp -f "{{root}}/target/release/aphrody_native.dll" "{{root}}/target/py/aphrody_native.pyd" 2>/dev/null || cp -f "{{root}}/target/release/libaphrody_native.so" "{{root}}/target/py/aphrody_native.so" 2>/dev/null || true

# Build Java 25 core (`crates/ay-core/jni` + `system/core-java`) and run FFM + JNI verification
[group('core')]
java-build:
    cargo build -p ay-core-jni --release
    javac --release 25 -d "{{root}}/target/core-java/classes" "{{root}}/system/core-java/src/main/java/com/aphrody/core/"*.java "{{root}}/system/core-java/src/test/java/com/aphrody/core/"*.java

[group('core')]
java-test: java-build
    java --enable-native-access=ALL-UNNAMED -cp "{{root}}/target/core-java/classes" com.aphrody.core.AphrodyCoreTest
