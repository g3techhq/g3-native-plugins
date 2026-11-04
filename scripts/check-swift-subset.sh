#!/usr/bin/env bash
# Builds the Swift package the way `dx` does for an app with only some plugins,
# and fails if the framework still references a permission API those plugins
# do not use. App Store Connect rejects such a binary (ITMS-90683).
#
# Usage: scripts/check-swift-subset.sh [Token...]
#   Tokens as in build.rs, default: tawny's set
#   (BackButton Clipboard ExternalUrl Media Storage).
# Needs macOS with Xcode.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
tokens=("$@")
if [[ ${#tokens[@]} -eq 0 ]]; then
  tokens=(BackButton Clipboard ExternalUrl Media Storage)
fi

product="G3NativePlugins$(printf '%s' "${tokens[@]}")"
directory="$(echo "$product" | tr '[:upper:]' '[:lower:]')"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# What dx does: copy the package into a directory named after the product in
# lower case, make the library dynamic, and build that one product.
cp -R "$root/src/ios" "$work/$directory"
sed -i '' 's/type: \.static/type: .dynamic/' "$work/$directory/Package.swift"
sdk="$(xcrun --sdk iphonesimulator --show-sdk-path)"
xcrun swift build \
  --package-path "$work/$directory" \
  --configuration release \
  --triple arm64-apple-ios-simulator \
  --sdk "$sdk" \
  --product "$product" \
  --build-path "$work/$directory/.build" \
  -Xswiftc -target -Xswiftc arm64-apple-ios17.5-simulator

# The binary itself, not the copy of it inside the .dSYM: debug info names
# every imported SDK type, used or not.
library="$(find "$work/$directory/.build" -type f -name "lib$product.dylib" -not -path '*.dSYM/*' | head -n 1)"
[[ -n "$library" ]] || { echo "no lib$product.dylib was built" >&2; exit 1; }
contents="$(strings -a "$library")"

# token: strings that only that plugin's Swift references.
declare -a checks=(
  "Auth:ASAuthorization"
  "CameraMicrophone:requestAccessForMediaType"
  "Geolocation:CLLocationManager"
  "Geolocation:requestWhenInUseAuthorization"
  "InAppPurchases:InAppPurchasesPlugin"
  "Notifications:UNUserNotificationCenter"
  "PushNotifications:registerForRemoteNotifications"
)
status=0
for check in "${checks[@]}"; do
  token="${check%%:*}"
  needle="${check#*:}"
  enabled=false
  for selected in "${tokens[@]}"; do
    [[ "$selected" == "$token" ]] && enabled=true
  done
  # Notifications are part of push.
  [[ "$token" == Notifications && " ${tokens[*]} " == *" PushNotifications "* ]] && enabled=true
  if [[ "$enabled" == false ]] && grep -q "$needle" <<<"$contents"; then
    echo "$product references $needle, which only $token uses" >&2
    status=1
  fi
done

for selected in "${tokens[@]}"; do
  grep -q "${selected}Plugin" <<<"$contents" || {
    echo "$product is missing ${selected}Plugin" >&2
    status=1
  }
done

# Each plugin's own block asks dx for its own product too (in a copy named
# after it). That one must not be a dynamic library, or dx could install it in
# place of the one above.
first="${tokens[0]}Plugin"
plugin_directory="$(echo "$first" | tr '[:upper:]' '[:lower:]')"
cp -R "$root/src/ios" "$work/$plugin_directory"
sed -i '' 's/type: \.static/type: .dynamic/' "$work/$plugin_directory/Package.swift"
xcrun swift build \
  --package-path "$work/$plugin_directory" \
  --configuration release \
  --triple arm64-apple-ios-simulator \
  --sdk "$sdk" \
  --product "$first" \
  --build-path "$work/$plugin_directory/.build" \
  -Xswiftc -target -Xswiftc arm64-apple-ios17.5-simulator
if find "$work/$plugin_directory/.build" -name "lib$first.dylib" | grep -q .; then
  echo "$first built a dynamic library, which dx could install instead of $product" >&2
  status=1
fi

[[ $status -eq 0 ]] && echo "$product holds only ${tokens[*]}; $first is not a dynamic library"
exit $status
