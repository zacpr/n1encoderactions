id := "net.ashurtech.n1-encoder-actions.sdPlugin"
plugin_name := "net.ashurtech.n1-encoder-actions.streamDeckPlugin"

release: bump package tag

package: build-linux collect zip

bump next=`git cliff --bumped-version | tr -d "v" 2>/dev/null || echo "0.1.1"`:
    git diff --cached --exit-code 2>/dev/null || true

    echo "We will bump version to {{next}}, press any key"
    read ans

    sed -i 's/"Version": ".*"/"Version": "{{next}}"/g' manifest.json
    sed -i 's/^version = ".*"$/version = "{{next}}"/g' Cargo.toml

tag next=`git cliff --bumped-version 2>/dev/null || echo "v0.1.0"`:
    echo "Generating changelog"
    git cliff -o CHANGELOG.md --tag {{next}} 2>/dev/null || echo "# Changelog" > CHANGELOG.md

    echo "We will now commit the changes, please review before pressing any key"
    read ans

    git add .
    git commit -m "chore(release): {{next}}" || true
    git tag "{{next}}"

build-linux:
    cargo build --release --target x86_64-unknown-linux-gnu

build-mac:
    docker run --rm -it -v $(pwd):/io -w /io ghcr.io/rust-cross/cargo-zigbuild:latest cargo zigbuild --release --target universal2-apple-darwin

build-win:
    cargo build --release --target x86_64-pc-windows-gnu

clean:
    rm -rf target/ build/

collect:
    rm -rf build
    mkdir -p build/{{id}}
    cp assets/icon.png build/{{id}}/icon.png 2>/dev/null || echo "No icon.png, skipping"
    cp manifest.json build/{{id}}/
    cp inspector.html build/{{id}}/ 2>/dev/null || echo "No inspector.html, skipping"
    cp target/x86_64-unknown-linux-gnu/release/n1encoderactions build/{{id}}/n1encoderactions
    @echo ""
    @echo "✓ Plugin files collected to: $(pwd)/build/{{id}}/"
    @echo ""

[working-directory: "build"]
zip:
    zip -r {{plugin_name}} {{id}}/
    @echo ""
    @echo "Build output dir: $(pwd)"
    @echo "✓ Plugin package created: $(pwd)/{{plugin_name}}"
    @echo ""
