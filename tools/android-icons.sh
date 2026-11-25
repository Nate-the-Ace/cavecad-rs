#!/bin/sh
# Makes the Android launcher icons (gen/android res/mipmap-*) from the CaveCAD logo, icons/source.png, the same
# source and the same full square as tools/ios-icons.sh (paper colour, grid lines carried to the edges).
#  - adaptive icon (Android 8+, what every supported phone uses): mipmap-anydpi-v26/ic_launcher(.round).xml =
#    the paper colour (values/ic_launcher_background.xml) behind ic_launcher_foreground.png (grid and logo on
#    transparent, 108 dp canvas; the logo sits inside the 66 dp safe circle, so no launcher mask cuts it)
#  - legacy icons: ic_launcher.png (the logo's own rounded square) and ic_launcher_round.png (a circle), 48 dp
# Run again after changing the logo. Needs ImageMagick (brew install imagemagick).
set -e
cd "$(dirname "$0")/../apps/desktop/src-tauri"
res=gen/android/app/src/main/res
paper='#f7f6f2'
full=$(mktemp -t cavecad-icon).png
magick -size 1024x1024 xc:"$paper" -fill '#e4e2da' \
  -draw 'rectangle 0,289 1023,294' -draw 'rectangle 0,729 1023,734' \
  -draw 'rectangle 289,0 294,1023' -draw 'rectangle 729,0 734,1023' \
  icons/source.png -composite -alpha remove -alpha off -depth 8 "PNG24:$full"
for d in mdpi:1 hdpi:1.5 xhdpi:2 xxhdpi:3 xxxhdpi:4; do
  name=${d%%:*}; scale=${d#*:}
  legacy=$(echo "48 * $scale" | bc | cut -d. -f1)
  fg=$(echo "108 * $scale" | bc | cut -d. -f1)
  mkdir -p "$res/mipmap-$name"
  magick icons/source.png -resize "${legacy}x${legacy}" "PNG32:$res/mipmap-$name/ic_launcher.png"
  magick "$full" -resize "${legacy}x${legacy}" \
    \( +clone -alpha transparent -fill white -draw "circle $((legacy/2)),$((legacy/2)) $((legacy/2)),0" \) \
    -alpha set -compose DstIn -composite "PNG32:$res/mipmap-$name/ic_launcher_round.png"
  inner=$(echo "82 * $scale" | bc | cut -d. -f1)   # the square at 82 of 108 dp: the logo's corners stay in the 66 dp safe circle
  magick "$full" -fuzz 2% -transparent "$paper" -resize "${inner}x${inner}" -background none -gravity center \
    -extent "${fg}x${fg}" "PNG32:$res/mipmap-$name/ic_launcher_foreground.png"
  echo "mipmap-$name legacy ${legacy}px, foreground ${fg}px"
done
rm -f "$full"
mkdir -p "$res/mipmap-anydpi-v26"
for n in ic_launcher ic_launcher_round; do
  cat > "$res/mipmap-anydpi-v26/$n.xml" <<'EOF'
<?xml version="1.0" encoding="utf-8"?>
<!-- Made by tools/android-icons.sh -->
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@color/ic_launcher_background" />
    <foreground android:drawable="@mipmap/ic_launcher_foreground" />
</adaptive-icon>
EOF
done
cat > "$res/values/ic_launcher_background.xml" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<!-- Made by tools/android-icons.sh: the logo's paper colour, behind the adaptive icon's foreground -->
<resources>
    <color name="ic_launcher_background">$paper</color>
</resources>
EOF
# the Android Studio template's own icon drawables are not used
rm -f "$res/drawable/ic_launcher_background.xml" "$res/drawable-v24/ic_launcher_foreground.xml"
rmdir "$res/drawable" "$res/drawable-v24" 2>/dev/null || true
