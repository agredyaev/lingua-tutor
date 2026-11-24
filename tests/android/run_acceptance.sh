#!/usr/bin/env bash
set -euo pipefail

apk="${1:?Pass an APK path}"
tag="${2:?Pass the expected version tag}"
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]
test -s "$apk"
evidence="$RUNNER_TEMP/android-evidence"
mkdir -p "$evidence"

sudo chmod a+rw /dev/kvm
for attempt in 1 2 3; do
  if sdkmanager --install "build-tools;35.0.0" "emulator" "system-images;android-35;google_apis;x86_64"; then
    break
  fi
  [[ "$attempt" -lt 3 ]] || exit 1
  sleep 10
done
"$ANDROID_HOME/build-tools/35.0.0/apksigner" verify --verbose --print-certs "$apk"
"$ANDROID_HOME/build-tools/35.0.0/aapt2" dump badging "$apk" > "$evidence/badging.txt"
grep -F "package: name='io.linguatutor'" "$evidence/badging.txt"
grep -F "versionName='${tag#v}'" "$evidence/badging.txt"
echo no | avdmanager create avd --force --name lingua-acceptance --package "system-images;android-35;google_apis;x86_64"
# Set the display before boot; resizing a running software-rendered WebView
# has produced invalid ColorBuffer errors and transient ADB disconnects.
config="$HOME/.android/avd/lingua-acceptance.avd/config.ini"
sed -i '/^hw.lcd.\(width\|height\|density\)=/d' "$config"
printf '\nhw.lcd.width=1080\nhw.lcd.height=1920\nhw.lcd.density=180\n' >> "$config"
"$ANDROID_HOME/emulator/emulator" -avd lingua-acceptance -no-window -no-audio -no-boot-anim \
  -no-snapshot -gpu swiftshader -no-metrics -memory 4096 -cores 4 > "$evidence/emulator.log" 2>&1 &
emulator_pid=$!
trap 'kill "$emulator_pid" 2>/dev/null || true' EXIT
timeout 180 adb wait-for-device
timeout 180 bash -c 'until [[ "$(adb shell getprop sys.boot_completed | tr -d "\r")" == 1 ]]; do sleep 2; done'
timeout 120 bash -c 'until adb shell pm path com.google.android.webview | grep -q package:; do sleep 3; done'
adb shell input keyevent 82
adb shell settings put system screen_off_timeout 1800000
adb shell svc power stayon true
sleep 15
adb shell settings put global window_animation_scale 0
adb shell settings put global transition_animation_scale 0
adb shell settings put global animator_duration_scale 0
adb shell wm size
adb shell wm density
adb shell getprop ro.build.version.sdk
adb shell dumpsys webviewupdate > "$evidence/webview.txt"
adb install "$apk"
python3 tests/android/release_acceptance.py "$tag"
