---
title: Profiling Sui on macOS using XCode Instruments
---

To profile on macOS:

1. Install XCode: https://apps.apple.com/us/app/xcode/id497799835?mt=12
1. Make sure you have the command line tools by running `xcode-select install`. If it says they are already installed, you may want to go directly to https://developer.apple.com/download/more/ and download the `.pkg` for the command line tools to make sure they are up to date.
1. Add `/Applications/Xcode.app/Contents/Developer/usr/bin` to your `$PATH`.
1. Build whatever Sui component you want to run normally with `cargo build` - these docs assume `target/debug/sui`.
1. Sign the binary you wish to profile:
    1. Create a file called `debug.plist` with the following contents:
       ```shell
          <?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>com.apple.security.get-task-allow</key><true/></dict></plist>
          ```
    1. Run:
       ```shell
          $ codesign -s - -v -f --entitlements ../debug.plist target/debug/sui
          ```
1. Run the app and record a trace (select the most appropriate template; see xcode documentation for available templates):
   ```shell
        $ xcrun xctrace record --template 'Allocations' --launch -- ./target/debug/sui start
        ```

1. Ctrl-C the app when you've recorded enough.
1. It will write a directory starting with `Launch_` - run:
   ```shell
      $ open Launch_xxxxxxxx
      ```

1. The trace should open in XCode.
