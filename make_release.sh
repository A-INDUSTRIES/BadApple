mkdir "$(pwd)/release/"
cp "$(pwd)/target/release/badapple.exe" "$(pwd)/release/"
cp "$(pwd)/BadApple.webm" "$(pwd)/release/"
cp "$(pwd)/BadApple.wav" "$(pwd)/release/"
zip -r ./release.zip ./release/
rm -r "$(pwd)/release/"