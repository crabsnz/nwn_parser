# NWN Combat Tracker

A real-time combat tracker for Neverwinter Nights that monitors log files and provides detailed damage statistics, buff tracking, and combat analysis.

## Features

### Combat Analysis
- **Real-time DPS tracking** - Monitor damage per second for all combatants
- **Encounter management** - Automatically detects and separates combat encounters
- **Damage breakdown** - View damage by type (slashing, fire, magical, etc.)
- **Attack analysis** - Track hit/miss/critical hit rates
- **Spell tracking** - Monitor spell casts, resists, and saves

### Buff Tracking
- **Divine spell tracking** - Automatically tracks Divine Might, Divine Shield, and other timed buffs
- **Buff expiration warnings** - Visual alerts when buffs are about to expire
- **Rest detection** - Automatically clears all buffs when resting
- **Configurable warnings** - Set custom warning times for expiring buffs

### Player Management
- **Auto-detection** - Automatically identifies players from chat and party messages
- **Character linking** - Links character names to account names
- **Main player tracking** - Tracks your main character for buff management
- **Player vs. NPC distinction** - Different colored bars for players and enemies

### UI Features
- **Draggable windows** - Move windows anywhere on screen
- **Persistent positioning** - Windows remember their positions
- **Multiple view modes** - Current fight, overall stats, or selected encounters
- **Filtering options** - Filter by damage type (done/taken) and combatant type
- **Scalable interface** - Adjust font size and zoom level

### Log Analysis
- **Live log monitoring** - Watches log files for real-time updates
- **Historical data** - Process entire log files for historical analysis
- **Log window** - View and filter recent log entries by type
- **Combat log filtering** - Filter by chat, combat rolls, damage, spell casting

## Installation

### Prerequisites
- Rust (latest stable version)
- Neverwinter Nights (for generating log files)

### Building from Source
```bash
git clone https://github.com/crabsnz/nwn_parser
cd nwn_parser
cargo build --release
```

### Running the Application
```bash
cargo run
```

The application will automatically:
1. Detect your NWN log directory
2. Start monitoring the latest log file
3. Display the main tracker window

## Configuration

### Settings
The application creates a `settings.json` file with the following options:

- **Caster Level** (1-40) - Used for spell duration calculations
- **Charisma Modifier** (-10 to +50) - Used for spell duration calculations
- **Extended Divine Might** - Whether you have the Extended Divine Might feat
- **Extended Divine Shield** - Whether you have the Extended Divine Shield feat
- **Buff Warning Seconds** (1-30) - How many seconds before expiration to show warnings
- **Log Directory** - Custom path to NWN log files (auto-detected by default)

### Log Directory Detection
The application automatically detects log files in these locations:

**Windows:**
- `Documents/Neverwinter Nights/logs`
- `Documents/Neverwinter Nights Enhanced Edition/logs`

**Linux/macOS:**
- `~/.local/share/Neverwinter Nights Enhanced Edition/logs`
- Various Steam and GOG installation paths

### Manual Log Directory
If auto-detection fails, you can manually set the log directory in the options panel.

## Usage

### Basic Operation
1. **Launch the application** - The tracker window will appear
2. **Join a server in NWN** - The application will detect your account
3. **Type in chat** - This activates buff tracking for your character
4. **Start combat** - Damage and statistics will appear automatically

### Windows

#### Main Window
- **Title bar** - Shows your account and character name, or "Type in chat to activate buffs"
- **View modes** - Switch between Current Fight, Overall Stats, or specific encounters
- **Filter options** - Toggle between damage done/taken and filter by player type
- **Minimize button** - Collapse the button rows for a smaller window

#### Buff Window
- **Always on top** - Stays visible over NWN
- **Auto-sizing** - Adjusts size based on number of active buffs
- **Drag anywhere** - No title bar, entire window is draggable
- **Color-coded warnings** - Buffs flash red when expiring soon
- **Persistent position** - Remembers where you place it

#### Logs Window
- **Real-time updates** - Shows last 50 log entries
- **Filtering** - Toggle chat, combat rolls, damage, spell casting, and other events
- **Full log view** - Load and filter the complete log file
- **Auto-scroll** - Automatically scrolls to newest entries

#### Player Details
- **Click any player** - Opens detailed statistics window for that player
- **Damage breakdown** - See damage by type and weapon
- **Timeline view** - View damage over time
- **Export options** - Copy statistics for analysis

### Buff Management
The application automatically tracks these divine spells:
- Divine Might (duration varies by caster level and charisma)
- Divine Shield (duration varies by caster level and charisma)
- Divine Power, Divine Favor, and other timed divine spells

**Configuration:**
1. Set your caster level in the options
2. Set your charisma modifier
3. Enable Extended Divine Might/Shield if you have those feats
4. Adjust warning time for buff expiration alerts

### Combat Analysis
- **Encounter Detection** - Combats are automatically separated by 6-second gaps
- **DPS Calculation** - Real-time damage per second for active encounters
- **Damage Types** - Track slashing, piercing, bludgeoning, fire, cold, electrical, etc.
- **Attack Success** - Monitor hit/miss ratios and critical hit frequency
- **Spell Analysis** - Track spell resists, saves, and damage output

### Data Persistence
- **Player Registry** (`players.json`) - Stores account/character mappings
- **Settings** (`settings.json`) - Stores user preferences and configuration
- **Auto-save** - All data is automatically saved when changed

## Troubleshooting

### Log Files Not Found
- Ensure NWN is generating log files (check game settings)
- Verify the log directory in the options panel
- Look for `.txt` files in your NWN installation's `logs` folder

### Player Not Detected
- Type something in chat while in-game
- Check that your account name appears in the title bar
- Restart the application if detection fails

### Buffs Not Tracking
- Ensure you've typed in chat to activate player detection
- Verify your caster level and charisma modifier are set correctly
- Check that you're casting divine spells (not arcane)

### Performance Issues
- Close unnecessary detail windows
- Restart the application periodically for long gaming sessions
- Check available disk space for log file processing

## Technical Details

### Architecture
- **Rust/egui** - High-performance GUI framework
- **Real-time parsing** - Efficient log file monitoring with minimal CPU usage
- **Regex-based parsing** - Fast pattern matching for combat events
- **Thread-safe design** - Separate threads for UI and log processing

### File Formats
- **Log parsing** - Processes NWN's standard combat log format
- **JSON storage** - Human-readable configuration and data files
- **Cross-platform** - Works on Windows, Linux, and macOS

### Data Safety
- **Non-intrusive** - Only reads log files, never modifies game files
- **Local storage** - All data stored locally, no network communication
- **Backup-friendly** - JSON files can be easily backed up or shared

## Contributing

### Development Setup
1. Install Rust (https://rustup.rs/)
2. Clone the repository
3. Run `cargo test` to run the test suite
4. Run `cargo run` to start the application

### Code Structure
- `src/main.rs` - Application entry point
- `src/gui/` - User interface components
- `src/parsing/` - Log file parsing and regex patterns
- `src/models/` - Data structures and game logic
- `src/utils/` - Utility functions and file I/O

### Adding Features
- New spell tracking can be added in `src/models/buffs.rs`
- UI components are in `src/gui/`
- Log parsing patterns are in `src/parsing/regex.rs`

## License

This project is licensed under the Apache License, Version 2.0.

```
Copyright 2024 NWN Combat Tracker Contributors

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

See the [LICENSE](LICENSE) file for the full license text.

## Support

For bugs, feature requests, or questions:
- Check the Issues tab on the repository
- Provide log files and settings.json when reporting bugs
- Include your OS and NWN version in bug reports
