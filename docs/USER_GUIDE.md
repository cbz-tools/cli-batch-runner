# User Guide

## Configure a CLI

Select a tab or use **Add tab** (+). Open **CLI Settings** and set a display name, full path to an existing executable, argument template, and optional working directory. **Browse** beside the executable selects `.exe` files. A blank working directory uses the executable's folder. Bare executable names are not searched on `PATH`, and environment variables in paths are not expanded.

Choose **Save** to validate the profile. **Cancel**, the header close button, or **Esc** discards changes. The application settings menu contains **Theme** and **About**. About links to the repository and release page.

Right-click a CLI tab to rename, duplicate, move, or delete it. The header also provides Add, Delete, Move Left, and Move Right. Deleting a tab removes its saved configuration and saved log history, not input files. The active job's profile is protected while running.

The tab strip stays on one row and scrolls horizontally when profiles do not fit. Use the tab bar's menu to see and select any profile. Long names are shortened with an ellipsis in the strip and shown in full in the menu.

## Arguments and Help

Open **Help / Examples** in CLI Settings. **Placeholders** lists all five placeholders with expansions for `C:\Files\Sample.txt`. **Examples** pairs executable paths with copyable CertUtil argument templates. Each **Copy** button copies its placeholder or argument template. Help scrolls, supports mouse text selection, and supports **Ctrl+A** / **Ctrl+C**. Use **Close** or **Esc** to dismiss it.

| Placeholder | Expansion |
|---|---|
| `{input}` | Full input file path |
| `{input_dir}` | Parent directory |
| `{input_name}` | File name including extension |
| `{input_stem}` | File name without extension |
| `{input_ext}` | Extension without the dot; empty if there is no extension |

Arguments are split before placeholders are expanded. Whitespace separates arguments; matching single or double quotes group text, and grouping quotes are removed. Inside double quotes, `\"` and `\\` become a literal quote and backslash; outside quotes, a backslash before a quote or another backslash escapes that character. An unmatched quote prevents saving or running. Unknown placeholders remain literal. Empty Arguments uses `{input}` as a single argument. Expanded paths stay within their original argument, including spaces and special characters.

The app invokes the executable directly. It does not interpret pipes, redirection, environment variables, or shell operators. If you explicitly run `cmd.exe /c` or PowerShell `-Command`, that program interprets the command text; CBR does not escape expanded paths for that shell.

The two Windows built-in CertUtil examples pass file paths as data without a shell. **SHA256 Hash** reads `{input}` and prints its hash. **Base64 Export** reads `"{input_dir}\{input_name}"` and writes `"{input_dir}\{input_stem}.{input_ext}.base64.txt"`. The output uses CertUtil's Base64 header and footer. Existing output files cause an error and are not overwritten; the input remains unchanged. For an extensionless `Sample`, the output name is `Sample..base64.txt`. Together, these examples use all five placeholders, including paths with spaces, Japanese characters, apostrophes, brackets, and shell metacharacters. See the [README examples](../README.md#usage--examples) for full templates and executable paths.

## Queue and Run

Use **Add Files** to select files or **Add Folder** for files directly inside a folder. Add Folder is not recursive. Drag files into the normal queue view, or paste clipboard file paths with **Ctrl+V**. Duplicate canonical paths are ignored. Settings mode does not accept file drops.

Newly added files are unselected. Select rows with clicks, **Ctrl**, **Shift**, or the keyboard. **Ctrl+click** toggles a row; click empty queue space to clear selection. **Ctrl+A** selects only visible rows. Clearing selection does not remove files. **Remove** removes selected rows and **Clear** empties the queue; neither deletes files from disk.

Use **Filter files...** in the File header to search displayed file names by case-insensitive substring. The Status filter is available from the queue header before and after execution; **ALL** shows every status. File and Status filters combine with AND, preserve each other's value, and changing either clears selection. Filtering changes only which rows are shown; it does not change queue order or files on disk. Active filters remain visually highlighted.

The file list shows a vertical scrollbar when the filtered rows exceed the viewport. Its reserved column stays in place when a scrollbar is not needed, so the File, Status, and Log columns keep their positions.

When the queue is empty, the column headers and filters are hidden and the queue shows **No files added yet.** The main action shows **All (0)** or **Selected (0)** according to the current tab choice, and cannot run; the result summary is hidden. If filters hide every registered file, the headers and filters remain visible with **No files match the current filters.** The main action remains disabled at zero, while the result summary still covers the full queue.

The bottom panel keeps progress or results in a fixed-height area above one action row. File controls stay grouped on the left. On the right, Job Log and a fixed 156 × 32 px split button stay together. Its 128 px main section runs the displayed target, and its 28 px refresh button switches between **All (N)** and **Selected (N)**. **All (N)** counts and runs every visible file; **Selected (N)** counts and runs selected visible files. Start and Stop share the same position and dimensions:

| Icon | Action |
|---|---|
| Play + All (N) / Selected (N) | Runs the displayed target in queue order; **Selected (0)** is disabled |
| Refresh | Switches between **All** and **Selected** without starting a job |
| Square + Stop | **Stop** — terminates the active child process and leaves remaining targets Pending; the refresh button is disabled |
| Scroll Text | **Job Log** — opens the latest run for the relevant tab |

The All/Selected choice is temporary and independent per tab. New tabs, duplicated tabs, and all tabs after an application restart begin in All. Changing filters or row selection does not change the choice. Application buttons have English tooltips. Unavailable actions use the theme's disabled styling. Open the Status filter to find results such as Ret1 or Success, then choose a target and run it. The All count always shows all visible files, even when rows are selected. Changing a filter (including returning to ALL) clears selection. Both targets are captured once; subsequent status changes do not change a run. A run starts only after its profile validation and log persistence succeed; at that point, every target is reset to Pending, its previous exit result and failure detail are cleared, and its previous File Log is detached. As RunnerEvents arrive, started targets show the current run's status and File Log. Selected rows remain selected during execution and afterward, so they can be run again. Only one batch runs at a time across all tabs. During execution and stopping, queue edits, selection, filters, and another Start are locked; Stop remains available until a stop is requested. Targets not reached before a Stop remain Pending without the previous File Log; rows outside the target keep their prior results and File Logs. CLI stdin is closed, and stdout and stderr are captured; configure tools for noninteractive operation.

Progress reports completed count, elapsed time, and current file on one line; long current file names are shortened to keep the layout steady. Exit code 0 appears as **Success (Ret0)**; other codes appear as **RetN**. Execution errors and stopped files have separate statuses. The main-screen result summary counts the latest completed statuses of every file in the current tab's queue, regardless of filters. It updates when rows are removed or rerun; new files start as Pending, and Clear removes the summary with the queue. By default, **On Non-Zero Exit** is **Continue**: a nonzero return does not abort remaining files. Choose **Stop** in CLI Settings to end the batch after RetN, leaving the remaining targets Pending. This setting is saved per profile; old settings default to Continue. Launch failures remain Error and keep their existing handling. Progress and Job Log counts describe the captured targets for this run. Long-path support also depends on the selected CLI.

## Logs and Settings

Select a row's file-document icon (tooltip: **View File Log**) to open that file's log; the icon is disabled until a log is available. Open **Job Log** for the batch log. Select text with the mouse, use **Ctrl+A** and **Ctrl+C**, or choose **Copy All**. Logs include paths, arguments, working directory, timestamps, duration, exit status, stdout, and stderr. Output is decoded as UTF-8 with replacement for invalid bytes; tools using another encoding may display incorrectly.

Settings: `%APPDATA%\cli-batch-runner\cli-batch-runner.toml` (Schema V1). Logs: `%APPDATA%\cli-batch-runner\logs\<process-id>\<tab-id>.json`. A tab keeps only the latest run by default. **Keep Job Log History** in CLI Settings is OFF by default; ON retains at most 20 runs, newest first, across restarts. Each profile has its own history at `%APPDATA%\cli-batch-runner\logs\history\<tab-id>.json`. The JSON array contains complete existing-format job records with unique job IDs, target files, timestamps, exit codes, and per-file stdout/stderr. It is replaced atomically after writes finish. An interrupted run retains its last saved snapshot and is labeled incomplete on restart. The queue itself is not restored.

Starting a run removes the oldest retained records beyond the limit; switching history OFF removes all but the latest record. Deleting a profile removes its saved history. Temporary process logs are still cleaned independently for exited processes. In Job Log, **Latest run** is selected initially; with history ON, the history menu also offers past runs with timestamps and job IDs. With history OFF, past runs cannot be selected.

Use **Export** in either log dialog to save exactly the currently displayed text as UTF-8 TXT. Job Log exports only the selected run; File Log exports only that file's displayed log. The asynchronous Windows save dialog handles the destination and overwrite confirmation. Cancel leaves files unchanged; write failures appear in the app's error banner. Copy, selection, and scrolling remain available. If APPDATA is unavailable, the app tries LOCALAPPDATA, the executable folder, and finally a temporary directory.

Existing V1 profile, tab-order, selection, and window-position fields are retained. An unreadable or newer-schema configuration disables saving to protect the file.

## Themes

Choose **Theme** from the header settings menu. The choice takes effect immediately and is saved in the Schema V1 TOML file. Older V1 files without a theme key, or files naming an unavailable theme, use **Default Light**.

- Default Light
- Default Dark
- Tokyo Night
- Catppuccin Mocha
- Everforest Dark
- Gruvbox Dark
- Solarized Light
- Flexoki Light

The six JSON theme files come from the official [GPUI Kit v0.7.3 catalog](https://github.com/longbridge/gpui-kit/tree/v0.7.3/themes). Retained definitions and colors are unchanged. See the [third-party notices](../THIRDPARTY_LICENSES.md) for the license and attribution.

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+A` | Select visible queue rows, or all text in open Help/Log |
| `Ctrl+C` | Copy selected queue paths, or selected Help/Log text |
| `Ctrl+V` | Add clipboard file paths to the queue |
| `Up` / `Down` | Move queue selection; hold `Shift` to extend |
| `Delete` | Remove selected queue items when no job is running |
| `Esc` | Close the topmost dialog, or discard CLI Settings changes |

Text inputs keep their normal shortcuts. Queue shortcuts are suppressed during dialogs and settings.
