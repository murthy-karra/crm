# Native mobile toolchain setup

**TOOLS, SDKS AND VIRTUAL-DEVICE BOOTS VERIFIED — 2026-09-12.**
Android Studio/JBR, Xcode and both development SDKs are installed. The license
requirements encountered during initial setup are resolved. One Android ARM64
phone emulator and one iPhone simulator booted successfully, then were shut down
sequentially. No CRM native app exists, and none has been built or tested.

This is the local setup brief for [mobile offline-first planning](../plans/MOBILE_OFFLINE_FIRST.md),
under D-001 and D-016. It owns workstation preparation evidence only; it does not
choose mobile HTTP/persistence contracts, minimum supported phone versions or
customer-data policy. Swift/SwiftUI and Kotlin/Jetpack Compose remain fixed.

## Initial workstation inventory

These are the observations before the authorized installation follow-up. The
installation record below supersedes the initial absence of Xcode/Android
Studio/JBR.

| Check actually run | Result |
|---|---|
| `sw_vers`, `uname -m`, `sysctl -n machdep.cpu.brand_string` | macOS 26.6.2, build 25G83; arm64; Apple M1 Max. |
| `sysctl -n hw.memsize`, `df -h /` | 64 GiB RAM; approximately 270 GiB available on the APFS volume. Space is a point-in-time observation, not reserved capacity. |
| `xcode-select -p` | `/Library/Developer/CommandLineTools`. |
| `xcrun --find swift`, `swift --version` | CLT Swift 6.3.3; target arm64-apple-macosx26.0. This proves a macOS compiler exists, not an iOS build environment. |
| `xcodebuild -version` | Fails: full Xcode required; active directory is Command Line Tools. `/usr/bin/xcodebuild` is present but is not evidence of installed Xcode. |
| `xcrun simctl list runtimes` | Fails: `simctl` unavailable. No simulator runtime was enumerated. |
| Standard application paths and Spotlight bundle search | No Xcode or Android Studio found in `/Applications`, `~/Applications`, or Spotlight results for their bundle identifiers. Unindexed/custom installations are not exhaustively excluded. |
| `command -v` for native tools | Homebrew exists at `/opt/homebrew/bin/brew`; `mas`, `adb`, `sdkmanager`, `avdmanager`, `emulator`, `gradle` and `kotlinc` absent from PATH. |
| `brew --version`; installed formula/cask inventory with auto-update disabled | Homebrew 6.0.22. No matching installed Android Studio/command-line tools, Java distributions, Gradle, Kotlin or `mas` packages found in the checked inventory. |
| `/usr/libexec/java_home -V`; Java directory listing | No Java runtime found. `/Library/Java/JavaVirtualMachines` exists and is empty. `/usr/bin/java` and `/usr/bin/javac` are system launchers, not installed JDKs. |
| Standard Android/JDK paths and environment lookup | No SDK/AVD found at `~/Library/Android/sdk`, `~/Android/Sdk`, `/opt/android-sdk`, `/opt/homebrew/share/android-commandlinetools` or `~/.android/avd`; no user JavaVirtualMachines directory found. `ANDROID_HOME`, `ANDROID_SDK_ROOT`, `JAVA_HOME` and `DEVELOPER_DIR` unset in the checked shell. |
| Native project-file search | No existing iOS/Android build project found in their planned repository directories. There is no project Gradle/JDK or Xcode version pin to satisfy yet. |

The machine meets the observed Android host OS, CPU, RAM and disk requirements.
No macOS upgrade or additional RAM/storage purchase is indicated by these checks.
Android documents macOS 12+, Apple M1 support, 16 GB for Studio plus Emulator and
32 GB recommended free space. [Official installation requirements](https://developer.android.com/studio/install).

No install/download or native build/test had been performed at initial inventory.
Physical device availability has not been inspected.
D-016 §6 records that APNs/FCM developer accounts exist; this brief does not
contradict that decision or infer new enrollment is needed.

## Initial installation follow-up — historical evidence

The authorized follow-up installed the stable Android Studio app without sudo,
global environment changes or helper links on PATH:

```sh
HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_INSTALL_CLEANUP=1 \
  /opt/homebrew/bin/brew install --cask --appdir="$HOME/Applications" \
  --no-binaries --require-sha android-studio
```

- Exit status **0**. Installed to
  `/Users/karrad/Applications/Android Studio.app`; Homebrew records
  `2026.1.4.7,quail4`. App bundle build is `AI-261.26222.65.2614.16204760`.
- Homebrew's download URL exactly matched the ARM download link read from Google's
  official download HTML:
  `https://edgedl.me.gvt1.com/android/studio/install/2026.1.4.7/android-studio-quail4-mac_arm.dmg`.
  The 1,475,129,822-byte archive's SHA-256 was independently computed as
  `bbd17ed0acc689cf88017083778537660e4a71480867fde875617fbda02a0aec`, matching
  the cask checksum. No credential-bearing download logs are stored here.
- Bundled `java -version` and `javac -version` succeeded: **25.0.3**, JBR build
  `25.0.3+-15898627-b508.16`. `file` confirms the Java executable is ARM64;
  the Studio launcher contains ARM64 and x86_64 slices.
- `codesign --verify --deep --strict` passed for the installed app.
- App launch reached Android Studio Setup Wizard. Optional usage reporting was
  declined. The Standard setup preview proposed `~/Library/Android/sdk`, 582 MB,
  Emulator, Build-Tools 36, SDK Platform 37.0, Platform-Tools and Sources for 37.0.
  These are observed wizard proposals, not an approved project SDK/minSdk pin.
- The next screen required **`android-sdk-license`** acceptance; the agent stopped
  without selecting Accept. SDK/emulator packages were not installed at this
  stage. This historical pause was resolved in the authorized continuation below.
- `command -v studio` still returns nothing; `xcode-select -p` remains
  `/Library/Developer/CommandLineTools`. Checked `JAVA_HOME`, `ANDROID_HOME`,
  `ANDROID_SDK_ROOT` and `DEVELOPER_DIR` remain unset.

The App Store was already signed in, showed Xcode **26.6** with **Redownload**,
and completed installation to `/Applications/Xcode.app` without entering
credentials. The App Store then showed **Open**. Account-identifying UI text is
intentionally omitted from this record.

| Xcode check after installation | Actual result |
|---|---|
| Command-local `DEVELOPER_DIR=... xcodebuild -version` | Exit 0: Xcode 26.6, build 17F113. |
| Command-local `xcodebuild -checkFirstLaunchStatus` | Exit 69; first-launch setup incomplete. |
| Command-local `xcodebuild -showsdks` | Exit 69: Xcode license agreements have not been accepted. |
| Command-local `xcrun simctl list runtimes` | Exit 69: same unaccepted-license requirement; no runtime availability claim. |
| Launch `/Applications/Xcode.app` | Displays **Xcode and Apple SDKs Agreement**; acceptance not performed. |
| `xcode-select -p` | Still `/Library/Developer/CommandLineTools`; no global switch performed. |

At this initial stage, Apple agreement acceptance and first-launch components
remained outstanding. No account was added or signing/team selection changed.
No license bypass was used. Available disk space after both IDE installations
was approximately 262 GiB. The following continuation supersedes this pause.

## Authorized SDK continuation — current evidence

The user explicitly authorized both SDK agreements and then stated Android's
agreement was already accepted. Fresh UI inspection found Android Studio and
Xcode at their Welcome windows. Android's default SDK was installed, and Xcode
`-checkFirstLaunchStatus` now returned **0**. No repeated agreement, credential
entry or privilege prompt was needed in this continuation.

| Component | Verified installation |
|---|---|
| Xcode | 26.6, build 17F113, `/Applications/Xcode.app`. |
| Apple SDKs | `xcodebuild -showsdks` passes and includes iOS/Simulator SDK 26.5. Other SDK headers bundled with Xcode were listed; no other platform simulator runtime was requested. |
| iOS simulator runtime | Explicit ARM64 iOS 26.5 download completed with exit 0: build 23F73. Final runtime inventory also contains build 23F77 and default devices; that second build appeared during the setup interval. No second runtime request was issued by this lane; the origin of the additional build was not independently verified. Neither runtime was removed. |
| Android Studio / JBR | Quail 4 / 2026.1.4.7; bundled ARM64 Java/Javac 25.0.3. |
| Android SDK root | `/Users/karrad/Library/Android/sdk`. |
| SDK platform and sources | `platforms/android-37.0` and `sources/android-37.0`, revision 2.0.0; retained from the user's completed default wizard. |
| Android Build-Tools | `build-tools/36.0.0`, version 36.0.0, retained from that default setup. No project build-tool/minSdk decision is made here. |
| Platform-Tools / adb | Platform-Tools 37.0.1; adb 1.0.41, build 37.0.1-15733141. |
| Android Emulator | 37.1.11.0, build 15917651. `-accel-check` passes with Hypervisor.Framework; no virtualization setting change required. |
| Command-Line Tools | `cmdline-tools/latest`, 23.0.0, installed using SDK Manager; component installer reported complete/finished. |
| Android CLI | 1.0.16261425, reached through the current Command-Line Tools distribution. |
| Android system image | `system-images/android-37.0/google_apis/arm64-v8a`, revision 6.0.0, installed from the stable channel; install exit 0. |
| Android phone AVD | `CRM_Field_API37_ARM64`, Pixel 9 profile, ARM64, created without overwrite/force. |

Android's current `sdkmanager` is a deprecated adapter for Android CLI. Initial
concurrent adapter checks triggered its first-use CLI bootstrap: one returned
`unknown (Android CLI)` and one failed with `No such file or directory`. These
were not successful package checks. Subsequent sequential verification of the
installed CLI succeeded, and a repeated `sdkmanager --version` reports
`1.0.16261425 (Android CLI)` with the deprecation notice. The CLI's supported
`--no-metrics` option and explicit SDK path were used for subsequent inventory
and the system-image installation. No extra agent skill, account or global
shell configuration was installed. [Current Android CLI documentation](https://developer.android.com/tools/agents/android-cli).

Commands actually used for the missing runtimes/tools included:

```sh
env DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  xcodebuild -downloadPlatform iOS -buildVersion 26.5 -architectureVariant arm64
env JAVA_HOME="$HOME/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  "$HOME/Library/Android/sdk/cmdline-tools/latest/bin/android" \
  --no-metrics --sdk="$HOME/Library/Android/sdk" sdk list
env JAVA_HOME="$HOME/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  "$HOME/Library/Android/sdk/cmdline-tools/latest/bin/android" \
  --no-metrics --sdk="$HOME/Library/Android/sdk" sdk install \
  system-images/android-37.0/google_apis/arm64-v8a
env JAVA_HOME="$HOME/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  "$HOME/Library/Android/sdk/cmdline-tools/latest/bin/avdmanager" create avd \
  --name CRM_Field_API37_ARM64 \
  --package 'system-images;android-37.0;google_apis;arm64-v8a' --device pixel_9
```

The Pixel 9 AVD cold-booted at `emulator-5554`, using the supported headless
emulator mode with audio/cameras disabled and no snapshot load. The startup log
reported **Boot completed in 27904 ms**; adb independently returned
`sys.boot_completed=1`, Android release **17**, API **37**, ABI **arm64-v8a** and
build **CE2A.260420.019**. The device was then shut down through `adb emu kill`
(exit 0), and the emulator process exited 0 before iOS boot began.

Nonfatal Android startup/shutdown observations: Vulkan 1.4 was not enabled for
the guest; emulator reported an absent update-check INI file, an unconfigured
emulator client at shutdown and a cancelled Netsim Wi-Fi stream. These did not
prevent the verified boot or clean shutdown. No CRM runtime was involved.

Xcode boot verification used the existing iPhone 17 device
`F5BF9C74-37AC-4546-A576-F30587CB4F4C`; no extra simulator was created.
`simctl boot` and `simctl bootstatus ... -b` completed with exit **0**. First boot
performed simulator data migration and reached **Finished** after **61 seconds**.
The simulator was then shut down with `simctl shutdown`; the final booted-device
list was empty. A diagnostic `sw_vers` spawned through simctl returned the host
macOS version and was not used as evidence of the guest runtime build.

Global `xcode-select` remains on Command Line Tools. Checked shell values for
`JAVA_HOME`, `ANDROID_HOME`, `ANDROID_SDK_ROOT` and `DEVELOPER_DIR` remain unset;
commands specify paths locally. No distribution signing, paid enrollment or
customer-data access was configured.
Final available disk space was approximately **227 GiB**. Documentation checks
found no broken local links, trailing whitespace or unmatched code fences.

## Xcode installation path

1. Install **Xcode 26.6 stable** in `/Applications/Xcode.app`. On the observation
   date, Apple's compatibility table lists Xcode 26.6 for macOS Tahoe 26.2–26.x;
   this workstation's 26.6.2 fits. Xcode 27 is listed as RC and is not the stable
   baseline selected here. Recheck the table immediately before installation if
   the date changes. This is a tool choice, not a minimum iPhone OS decision.
   [Apple system requirements](https://developer.apple.com/xcode/system-requirements).
2. Use the [Mac App Store Xcode listing](https://apps.apple.com/us/app/xcode/id497799835)
   reached from Apple's [Xcode resources](https://developer.apple.com/xcode/resources/).
   If that listing has moved to a different release, use Apple's linked developer
   downloads to obtain the compatible stable version instead. Developer downloads
   require Apple Account sign-in; paid program membership is not required to
   obtain them. No `mas` installation is necessary for the supported GUI route.
3. Launch Xcode, complete first-launch components and the presented license flow,
   and install one compatible iOS simulator runtime in Xcode's component settings.
   Record exact Xcode build, SDK and runtime versions after completion. Do not
   install every Apple platform runtime for this iPhone proof.
   [Apple component installation](https://developer.apple.com/documentation/xcode/downloading-and-installing-additional-xcode-components).
4. Prefer a command-local `DEVELOPER_DIR` initially. The installed
   `xcode-select(1)` manual confirms this overrides global selection without
   superuser permissions. This keeps the existing Rust/Web development tool
   selection stable while native setup is checked.

All four inventory commands below now **pass** after the authorized continuation.
The earlier license errors are preserved above as historical setup evidence:

```sh
env DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild -version
env DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild -showsdks
env DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcrun simctl list runtimes
env DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcrun simctl list devices available
```

Success means the expected stable build and iOS SDK are reported and at least one
available iPhone simulator can boot. Then compile and run the eventual scaffold
and its persistence test on that simulator; tool version output alone does not
complete native verification.

Simulator work does not require registering a physical device or preparing App
Store distribution. Physical iPhone testing needs the device, pairing/developer
setup and an Apple Account/team selected in Xcode for signing. Apple permits
limited personal device testing with a free Personal Team; distribution and
advanced capabilities have separate membership requirements. Verify the existing
team when those capabilities are in scope; do not infer a purchase is needed.
[Apple account and testing guidance](https://developer.apple.com/help/account/basics/about-your-developer-account),
[membership comparison](https://developer.apple.com/support/compare-memberships/).

## Android Studio, JDK and SDK installation path

1. Install **Android Studio Quail 4 / 2026.1.4, Mac with Apple chip**, from the
   [official download page](https://developer.android.com/studio). The
   [release page](https://developer.android.com/studio/releases) identifies Quail 4
   as the current stable channel release on the observation date. Use the ARM
   installer, `android-studio-quail4-mac_arm.dmg`, and the published verification
   information available at download time. Recheck if installation happens later.
2. The app is installed in `~/Applications/Android Studio.app`, and its Setup
   Wizard has completed. Keep the SDK in the standard user location
   `~/Library/Android/sdk` and record the actual path. Future downloads may present license terms;
   handle any account-owner interaction when actually encountered. No Google
   Play publication or production signing setup is part of this first proof.
   [Official Mac installation steps](https://developer.android.com/studio/install).
3. Use Studio's bundled **JetBrains Runtime (JBR)** to run the IDE. A standalone
   Homebrew Java installation is not a prerequisite for this route. When the
   Android project is created, pin the compatible Android Gradle Plugin, Gradle
   wrapper, Kotlin and build JDK together. Use the same compatible JDK for IDE
   Gradle builds and command-line builds. The eventual bundled JBR version and
   its compatibility must be checked, not guessed from Studio's version number.
   [Android JDK guidance](https://developer.android.com/build/jdks).
4. In SDK Manager, install Platform Tools, Command-Line Tools, Emulator, and the
   stable SDK platform/build tools required by the scaffold. Record their exact
   package IDs and versions. Select one compatible ARM64 phone system image in
   Device Manager, create an AVD and boot it. The app's `minSdk` and supported
   device range remain decisions for the mobile specification; installing a
   current SDK does not decide them. [SDK Manager](https://developer.android.com/studio/intro/update),
   [virtual device setup](https://developer.android.com/studio/run/managing-avds).

The Java, adb, emulator and AVD-list commands below now **pass**. Use the modern
CLI for SDK inventory because this version's `sdkmanager` is a compatibility
adapter. The app path reflects the actual user Applications installation:

```sh
"$HOME/Applications/Android Studio.app/Contents/jbr/Contents/Home/bin/java" -version
"$HOME/Library/Android/sdk/platform-tools/adb" version
"$HOME/Library/Android/sdk/emulator/emulator" -version
"$HOME/Library/Android/sdk/emulator/emulator" -list-avds
env JAVA_HOME="$HOME/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
  "$HOME/Library/Android/sdk/cmdline-tools/latest/bin/android" \
  --no-metrics --sdk="$HOME/Library/Android/sdk" sdk list
```

After the owned scaffold exists, record `./gradlew --version`, build its debug
app, run local tests and instrumented persistence tests on the booted AVD. Use
the project wrapper; a global Gradle/Kotlin installation is unnecessary. Physical
Android tests later need a device and authorized debugging connection. Emulator
success cannot stand in for real-device restart or poor-cellular evidence.

## Completion and parallel execution

This workstation setup task verifies installed tools/SDKs and a bootable virtual
phone on each platform. Building, launching and testing the first CRM native
scaffolds belongs to their implementation tasks; no such projects exist yet.
Record those app-specific commands and results in the owning native verification
records. SDK inventory and successful device boot do not prove CRM functionality
or offline durability.

One coordinator installs/updates shared tools. Each implementation lane owns its
simulator/AVD, app data and build output; do not reset another lane's device.
Keep native tests on isolated synthetic operational data. SQLite durability,
lost-response synchronization, sign-out behavior and real-device testing belong
to the mobile slice acceptance matrix, not a successful IDE installation.

Backend contract planning can proceed during setup. There is no evidence here
that an account purchase, OS upgrade or physical device is required before that
planning or the first simulator/emulator work begins.
