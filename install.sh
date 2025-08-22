TARGET=aarch64-unknown-linux-musl

cross build --release --target ${TARGET}

mv target/${TARGET}/release/tt ~/dev/linaro-cca/tee-tests