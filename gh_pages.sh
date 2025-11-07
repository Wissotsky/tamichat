dx bundle --out-dir docs --release
cp -r docs/public/* docs
rm -rf docs/public
cp docs/index.html docs/404.html
