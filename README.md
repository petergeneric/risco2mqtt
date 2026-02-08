# risco2mqtt

This program is based on a Rust rewrite of the TypeScript [risco-lan-bridge](https://github.com/vanackej/risco-lan-bridge/), built for more resource-constrained environments. It is intended to be used as a "bridge" between an application communicating via MQTT and a Risco alarm Panel (can also work with Electronic Line products but this has not been tested).

## Compatibility
Compatible with these central units and these features:
||Agility|Wicomm|Wicomm Pro|LightSYS|ProSYSPlus/GTPlus|Other|
|--|:--:|:--:|:--:|:--:|:--:|:--:|
|Zones|Y*|Y*|Y*|Y|Y*|?|
|Partitions|Y*|Y*|Y*|Y|Y*|?|
|Outputs|Y*|Y*|Y*|Y|Y*|?|
|Groups|N**|N**|N**|N**|N**|?|
|Arming|Y*|Y*|Y*|Y|Y*|?|
|Stay Arming|Y*|Y*|Y*|Y|Y*|?|
|Temporised Arming|N**|N**|N**|N**|N**|?|
|Disarming|Y*|Y*|Y*|Y|Y*|?|
|Bypass/UnBypass Zones|Y*|Y*|Y*|Y|Y*|?|
|Command Outputs|Y*|Y*|Y*|Y|Y*|?|
|PirCam Support|N***|N***|N***|N|N***|?|

*=> Theoretical compatibility not tested.

**=> Not functional today. 

***=> Not functional today (planned for a future version).

***WARNING : For control panels equipped with a Mono-Socket type IP module (IPC / RW132IP), direct connection may not work if RiscoCloud is enabled in the configuration.
To use this module, you must therefore deactivate RiscoCloud and then restart your control panel (or the IP module by deactivating it and then reactivating it in the configuration from the keyboard).
This action will prevent the control panel from connecting to RiscoCloud and you will no longer be able to use it remotely from the iRisco application.***


### License
risco2mqtt is licensed under the MIT license, so you can dispose of it as you see fit under the terms of that license.
