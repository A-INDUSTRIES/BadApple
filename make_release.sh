mkdir "$(pwd)/release/"
cp "$(pwd)/target/release/badapple.exe" "$(pwd)/release/"
cp "$(pwd)/BadApple.webm" "$(pwd)/release/"
for FILE in "$(pwd)"/src/dlls/*.*; do cp "$FILE" "$(pwd)/release/"; done
zip -r ./release.zip ./release/
rm -r "$(pwd)/release/"