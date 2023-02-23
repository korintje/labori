# labori
Data collection system from frequency counters

# Status
WIP

# Overview
![1](https://user-images.githubusercontent.com/30950088/220891137-2a06664d-d566-4330-b788-8ed843517006.png)

# Wrapper API
- Protocol: TCP/IP 
- Media type: JSON
## Commands
- Get{ key: x }
- Set{ key: x, value: y}
- Run{}
- RunExt{ duration: String }
- RunMulti{ channel_count: integer, switch_delay: float, channel_interval: float, interval: float }
- Stop{}

## Responses
### Success
- Finished(String)
- SaveTable(String)
- GotValue(String)
- SetValue(String)

### Failure
- Busy{table_name: String, interval: String},
- NotRunning(String)
- ErrorInRunning(String)
- InvalidRequest(String)
- InvalidReturn(String)
- InvalidCommand(String)
- CommandNotSent(String)
- PollerCommandNotSent(String)
- SaveDataFailed(String)
- MachineNotRespond(String)
- SignalFailed(String)
- SendToFrontFailed(String)
- EmptyStream(String)

# Usage
WIP
