# nist-time-sync

![Build linux](https://github.com/aazev/nist-time-sync/actions/workflows/linux.yml/badge.svg)
![Build Windows](https://github.com/aazev/nist-time-sync/actions/workflows/windows.yml/badge.svg)

# Purpose

When you dual boot linux and windows, and due the fact that ubuntu uses GMT for the system clock, windows doesn't update tie date and time correctly. And in the case of win 11, it even fails to properly update automatically.
This small application solves that, acting as a windows service, it synchronizes the system time every 60 minutes with a NIST Internet Time Server.

## TODO

### Windows

[ ] - Auto install as windows service
[x] - Start as a service
[x] - Sync time
[ ] - Stop service

### Linux

[x] - Sync time
[x] - Loop sync until stop signal
