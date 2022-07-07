---
title: Profiling Sui on Mac OS using XCode instruments.
---

To profile on Mac OS:

* Install XCode: https://apps.apple.com/us/app/xcode/id497799835?mt=12
* Make sure you have the command line tools but running `xcode-select install`. If it says they are already installed you may want to go directly to https://developer.apple.com/download/more/ and download the .pkg for the commandline tools to make sure they are up to date.
* Add `/Applications/Xcode.app/Contents/Developer/usr/bin` to your `$PATH`.
* Build whatever Sui component you want to run normally with `cargo build` - these docs assume `target/debug/sui`.
* Sign the binary you wish to profile:
    * Create a file called `debug.plist` with the following contents:

                <?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>com.apple.security.get-task-allow</key><true/></dict></plist>

    * Run:
            $ codesign -s - -v -f --entitlements ../debug.plist target/debug/sui
* Now run the app and record a trace (select the most appropriate template, see xcode documentation for available templates).

        $ xcrun xctrace record --template 'Allocations' --launch -- ./target/debug/sui start

* Ctrl-C the app when you've recorded enough.
* It will write a directory starting with `Launch_` - run:

        $ open Launch_xxxxxxxx

* The trace should open in XCode.
