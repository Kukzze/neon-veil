//! The 30 campaign quests across five tiers.

use crate::quest_engine::{QuestDefinition, QuestDifficulty};
use crate::skills::SkillType::{
    self, Agility, Ballistics, CyberMelee, Endurance, Engineering, Netrunning, Shadow, StreetCredit,
};

use QuestDifficulty::{Experienced, Grandmaster, Intermediate, Master, Novice};

const ALL_SKILLS_88: &[(SkillType, u8)] = &[
    (Agility, 88),
    (CyberMelee, 88),
    (Ballistics, 88),
    (Netrunning, 88),
    (Engineering, 88),
    (Shadow, 88),
    (Endurance, 88),
    (StreetCredit, 88),
];

const ALL_SKILLS_750K: &[(SkillType, u32)] = &[
    (Agility, 750_000),
    (CyberMelee, 750_000),
    (Ballistics, 750_000),
    (Netrunning, 750_000),
    (Engineering, 750_000),
    (Shadow, 750_000),
    (Endurance, 750_000),
    (StreetCredit, 750_000),
];

pub static MASTER_QUEST_CATALOG: &[QuestDefinition] = &[
    // ---- Tier 1: Novice (levels 1-20) ----
    QuestDefinition {
        id: 1,
        title: "Protocol Zero",
        difficulty: Novice,
        skill_requirements: &[],
        quest_prerequisites: &[],
        quest_giver: "Viktor Vance (Undercity Clinic)",
        district: "Undercity Slums",
        synopsis: "Wake up with wiped neural telemetry, calibrate basic parkour locomotion, and escape clinic under lockdown.",
        steps: &[
            "Calibrate cyber-eye",
            "Sprint and slide under security shutter",
            "Vault low debris",
            "Jump toxic canal",
            "Retrieve smart pistol",
            "Defeat security drone",
            "Exit to Neon Alley",
        ],
        failure_conditions: &["Taking lethal toxic waste damage"],
        xp_rewards: &[(Agility, 450), (Ballistics, 300)],
        credit_reward: 1_200,
        item_rewards: &["Stalker Trenchcoat"],
    },
    QuestDefinition {
        id: 2,
        title: "Neon Courier",
        difficulty: Novice,
        skill_requirements: &[(Agility, 5)],
        quest_prerequisites: &[1],
        quest_giver: "Dex the Dispatcher (Undercity Docks)",
        district: "Undercity Slums",
        synopsis: "Deliver encrypted data drive across rooftops within a 180-second timer without touching the street floor.",
        steps: &[
            "Receive courier satchel",
            "Climb fire escape",
            "Sprint across pipe bridge",
            "Wall-run billboard span",
            "Slide under steam vent",
            "Deliver to Kuroshio runner",
        ],
        failure_conditions: &["Touching ground level", "Timer expiring"],
        xp_rewards: &[(Agility, 1_200)],
        credit_reward: 2_500,
        item_rewards: &["Acrobat Pneumatic Boots"],
    },
    QuestDefinition {
        id: 3,
        title: "Scrap Metal Symphony",
        difficulty: Novice,
        skill_requirements: &[(Engineering, 5)],
        quest_prerequisites: &[1],
        quest_giver: "Anya Rust (Scrap Yard)",
        district: "Undercity Slums",
        synopsis: "Salvage 8 intact power capacitors from disabled street enforcers and craft a modded shock baton.",
        steps: &[
            "Loot scrap yard cache",
            "Salvage 8 capacitors",
            "Assemble at workbench",
            "Test shock baton on combat dummy",
        ],
        failure_conditions: &["Dying in scrap yard arena"],
        xp_rewards: &[(Engineering, 1_500), (CyberMelee, 800)],
        credit_reward: 3_000,
        item_rewards: &["Blueprint: Overcharged Shock Baton"],
    },
    QuestDefinition {
        id: 4,
        title: "Subnet Whispers",
        difficulty: Novice,
        skill_requirements: &[(Netrunning, 8)],
        quest_prerequisites: &[1],
        quest_giver: "Ghost_0x (Darknet Terminal)",
        district: "Kuroshio Commercial Hub",
        synopsis: "Infiltrate Kuroshio transit relay, breach 3 firewall nodes, and isolate syndicate packet taps.",
        steps: &[
            "Access maintenance access panel",
            "Solve matrix intrusion puzzle",
            "Bypass security proxy",
            "Download corrupted data core",
        ],
        failure_conditions: &["Failing intrusion puzzle 3 times (triggers lockdown)"],
        xp_rewards: &[(Netrunning, 2_200)],
        credit_reward: 4_200,
        item_rewards: &["Cyberdeck Mod: Buffer Overflow I"],
    },
    QuestDefinition {
        id: 5,
        title: "Blood on the Cobbles",
        difficulty: Novice,
        skill_requirements: &[(CyberMelee, 10)],
        quest_prerequisites: &[1],
        quest_giver: "Bouncer Kroll (Club Glitch)",
        district: "Undercity Slums",
        synopsis: "Eliminate an unruly squad of Iron Fang gang enforcers using only parries and melee finishers.",
        steps: &[
            "Enter back alley behind Club Glitch",
            "Perform 3 perfect parries against brawlers",
            "Execute finisher on gang lieutenant",
        ],
        failure_conditions: &["Firing ranged weapons", "Taking excessive damage"],
        xp_rewards: &[(CyberMelee, 3_000), (Endurance, 1_800)],
        credit_reward: 5_000,
        item_rewards: &["Tanto of the Fang"],
    },
    QuestDefinition {
        id: 6,
        title: "Shadow Walk",
        difficulty: Novice,
        skill_requirements: &[(Shadow, 12)],
        quest_prerequisites: &[1],
        quest_giver: "Silent Mira (Kuroshio Market)",
        district: "Kuroshio Commercial Hub",
        synopsis: "Sneak through a biometric sensor grid without sounding alarms or killing guards.",
        steps: &[
            "Disable rooftop sensor terminal",
            "Crouch-walk through vent shaft",
            "Steal shipment manifest",
            "Exfiltrate through skylight",
        ],
        failure_conditions: &["Triggering any active alarm sensor"],
        xp_rewards: &[(Shadow, 3_500)],
        credit_reward: 6_000,
        item_rewards: &["Optical Dampener Cloak"],
    },
    // ---- Tier 2: Intermediate (levels 20-45) ----
    QuestDefinition {
        id: 7,
        title: "High-Rise Heist",
        difficulty: Intermediate,
        skill_requirements: &[(Agility, 25), (Netrunning, 20)],
        quest_prerequisites: &[2, 4],
        quest_giver: "Viktor Vance (Kuroshio Penthouse)",
        district: "Kuroshio Commercial Hub",
        synopsis: "Scale the facade of Kuroshio Commercial Tower via billboard wall-runs to extract corporate ledger.",
        steps: &[
            "Scale exterior awning",
            "Wall-run holographic sign sequence",
            "Hack window seal",
            "Extract encrypted SSD",
            "Parachute to street",
        ],
        failure_conditions: &["Falling off building exterior"],
        xp_rewards: &[(Agility, 8_500), (Netrunning, 6_000)],
        credit_reward: 15_000,
        item_rewards: &[],
    },
    QuestDefinition {
        id: 8,
        title: "Corrosive Horizon",
        difficulty: Intermediate,
        skill_requirements: &[(Ballistics, 28), (Engineering, 25)],
        quest_prerequisites: &[3],
        quest_giver: "Chief Miller (Foundry Outpost)",
        district: "Industrial Foundry Sector",
        synopsis: "Investigate chemical leaks in Industrial Sector 4 and eliminate mutated biomechanical hounds.",
        steps: &[
            "Investigate vat room",
            "Repel 3 hound waves",
            "Repair ruptured conduit",
            "Collect acid sample",
        ],
        failure_conditions: &["Acid shield depletion"],
        xp_rewards: &[(Ballistics, 10_000), (Engineering, 8_000)],
        credit_reward: 0,
        item_rewards: &["Blueprint: Acid Spitter Shotgun"],
    },
    QuestDefinition {
        id: 9,
        title: "Neural Feedback",
        difficulty: Intermediate,
        skill_requirements: &[(Netrunning, 30)],
        quest_prerequisites: &[4],
        quest_giver: "Nyx (Data Vault 09)",
        district: "Kuroshio Commercial Hub",
        synopsis: "Counter-hack an enemy netrunner attempting to fry your cerebral implants in real-time.",
        steps: &[
            "Jack into counter-offensive rig",
            "Trace hostile ISP signal",
            "Neutralize 4 ice nodes",
            "Overload rival deck",
        ],
        failure_conditions: &["Memory buffer saturation"],
        xp_rewards: &[(Netrunning, 14_000)],
        credit_reward: 18_000,
        item_rewards: &["Rare Mod: Neuro-Firewall Prime"],
    },
    QuestDefinition {
        id: 10,
        title: "The Ronin's Debt",
        difficulty: Intermediate,
        skill_requirements: &[(CyberMelee, 32), (Agility, 25)],
        quest_prerequisites: &[5],
        quest_giver: "Master Jin (Rooftop Dojo)",
        district: "Kuroshio Commercial Hub",
        synopsis: "Duel three rogue cyber-samurai across high-voltage skybridge girders.",
        steps: &[
            "Meet Jin at sunrise",
            "Defeat apprentice Raizo",
            "Duel twin blades Ren & Kai",
            "Claim honorable blade",
        ],
        failure_conditions: &["Knocked off skybridge"],
        xp_rewards: &[(CyberMelee, 16_000), (Agility, 10_000)],
        credit_reward: 0,
        item_rewards: &["Crimson Muramasa Blade"],
    },
    QuestDefinition {
        id: 11,
        title: "Blackout Protocol",
        difficulty: Intermediate,
        skill_requirements: &[(Engineering, 35), (Shadow, 30)],
        quest_prerequisites: &[6],
        quest_giver: "The Saboteur (Undercity Substation)",
        district: "Undercity Slums",
        synopsis: "Plant EMP charges on 4 auxiliary generators supplying power to the Corporate Spire.",
        steps: &[
            "Craft 4 timed EMP satchels",
            "Infiltrate substation sub-level",
            "Plant charges without combat detection",
            "Detonate from vantage",
        ],
        failure_conditions: &["Combat alert raised before planting all 4 charges"],
        xp_rewards: &[(Engineering, 18_000), (Shadow, 12_000)],
        credit_reward: 22_000,
        item_rewards: &[],
    },
    QuestDefinition {
        id: 12,
        title: "Syndicate Shakedown",
        difficulty: Intermediate,
        skill_requirements: &[(StreetCredit, 38)],
        quest_prerequisites: &[1],
        quest_giver: "Boss Callahan (The Red Room)",
        district: "Undercity Slums",
        synopsis: "Intimidate 3 corrupt district merchants to align supply chains with Cutting Onion Syndicate.",
        steps: &[
            "Confront Kuroshio arms dealer",
            "Intercept illicit black market shipment",
            "Extort protection tribute",
        ],
        failure_conditions: &["Killing merchant targets"],
        xp_rewards: &[(StreetCredit, 22_000)],
        credit_reward: 25_000,
        item_rewards: &["Title: The Extorter"],
    },
    QuestDefinition {
        id: 13,
        title: "Biomod Overload",
        difficulty: Intermediate,
        skill_requirements: &[(Endurance, 40)],
        quest_prerequisites: &[1],
        quest_giver: "Dr. Zhao (Biotech Labs)",
        district: "Industrial Foundry Sector",
        synopsis: "Survive an experimental synthetic adrenaline injection while clearing waves of combat stim fiends.",
        steps: &[
            "Receive injection",
            "Defeat 20 stim fiends while health continuously drains",
            "Reach stabilizer chamber",
        ],
        failure_conditions: &["Health reaching 0 before chamber"],
        xp_rewards: &[(Endurance, 25_000)],
        credit_reward: 0,
        item_rewards: &["Permanent +20 HP", "Biomod Slot Upgrade"],
    },
    QuestDefinition {
        id: 14,
        title: "The Velocity Express",
        difficulty: Intermediate,
        skill_requirements: &[(Agility, 42)],
        quest_prerequisites: &[2],
        quest_giver: "Speed Demon Twitch (Monorail Depot)",
        district: "Kuroshio Commercial Hub",
        synopsis: "Sprint along the roof of a moving hyper-train, jumping obstacles and traversing couplings.",
        steps: &[
            "Board moving train roof",
            "Dodge low bridge gantries",
            "Vault cargo containers",
            "Infiltrate lead engine cabin",
        ],
        failure_conditions: &["Falling off speeding train"],
        xp_rewards: &[(Agility, 30_000)],
        credit_reward: 28_000,
        item_rewards: &["Kinetic Overdrive Boots"],
    },
    // ---- Tier 3: Experienced (levels 45-70) ----
    QuestDefinition {
        id: 15,
        title: "Ghost in the Spire",
        difficulty: Experienced,
        skill_requirements: &[(Shadow, 50), (Netrunning, 45)],
        quest_prerequisites: &[7, 9],
        quest_giver: "Nyx (Safehouse Omega)",
        district: "The Spire Megatowers",
        synopsis: "Infiltrate the Spire executive suites without leaving a digital footprint or being detected by drones.",
        steps: &[
            "Ascend exterior maintenance cable",
            "Breach laser sensor array",
            "Download biometric keys",
            "Evacuate via skybridge",
        ],
        failure_conditions: &["Raising alarm tier above Green"],
        xp_rewards: &[(Shadow, 45_000), (Netrunning, 38_000)],
        credit_reward: 45_000,
        item_rewards: &[],
    },
    QuestDefinition {
        id: 16,
        title: "Heavy Metal Thunder",
        difficulty: Experienced,
        skill_requirements: &[(Ballistics, 52), (Endurance, 48)],
        quest_prerequisites: &[8],
        quest_giver: "General Briggs (Fortified Bunker)",
        district: "Industrial Foundry Sector",
        synopsis: "Repel a full-scale corporate mechanized battalion using heavy railguns and anti-tank ordnance.",
        steps: &[
            "Man defenses",
            "Destroy 4 Mecha Enforcers",
            "Snipe targeting arrays on aerial gunship",
            "Secure perimeter",
        ],
        failure_conditions: &["Bunker core structural collapse"],
        xp_rewards: &[(Ballistics, 50_000), (Endurance, 35_000)],
        credit_reward: 0,
        item_rewards: &["Blueprint: Titan-Buster Railgun"],
    },
    QuestDefinition {
        id: 17,
        title: "The Razor's Edge",
        difficulty: Experienced,
        skill_requirements: &[(CyberMelee, 55), (Agility, 50)],
        quest_prerequisites: &[10],
        quest_giver: "Master Jin (Rooftop Dojo)",
        district: "The Spire Megatowers",
        synopsis: "Ascend an exterior crane during a hurricane, fighting cyber-ninjas on 20cm balance beams.",
        steps: &[
            "Climb swaying crane mast",
            "Fight 4 ninjas simultaneously on girders",
            "Duel Grandmaster Kage",
            "Claim katana hilt",
        ],
        failure_conditions: &["Falling into abyss"],
        xp_rewards: &[(CyberMelee, 60_000), (Agility, 45_000)],
        credit_reward: 0,
        item_rewards: &["Thunder God Muramasa"],
    },
    QuestDefinition {
        id: 18,
        title: "Dark Web Cartel",
        difficulty: Experienced,
        skill_requirements: &[(StreetCredit, 58), (Netrunning, 55)],
        quest_prerequisites: &[12],
        quest_giver: "Boss Callahan",
        district: "Kuroshio Commercial Hub",
        synopsis: "Hijack the Grand Exchange encrypted routing ledger to seize illicit corporate shipments.",
        steps: &[
            "Tap core optical conduit",
            "Reroute 3 cargo transports",
            "Siphon 100,000 credits",
            "Wipe routing logs",
        ],
        failure_conditions: &["Corporate trace reaching 100%"],
        xp_rewards: &[(StreetCredit, 70_000), (Netrunning, 50_000)],
        credit_reward: 0,
        item_rewards: &["Permanent 0.5% Grand Exchange Fee Reduction"],
    },
    QuestDefinition {
        id: 19,
        title: "Chrono-Lockdown",
        difficulty: Experienced,
        skill_requirements: &[(Netrunning, 60), (Engineering, 55)],
        quest_prerequisites: &[15],
        quest_giver: "Dr. Aris (Quantum Research Facility)",
        district: "The Spire Megatowers",
        synopsis: "Freeze time-dilation subroutines in a glitching military server facility.",
        steps: &[
            "Navigate chronally unstable chambers",
            "Synchronize 3 temporal relays",
            "Destroy rogue quantum AI core",
        ],
        failure_conditions: &["Caught in temporal singularity"],
        xp_rewards: &[(Netrunning, 80_000), (Engineering, 65_000)],
        credit_reward: 0,
        item_rewards: &["Chrono-Stabilizer Cyberdeck"],
    },
    QuestDefinition {
        id: 20,
        title: "Foundry Cataclysm",
        difficulty: Experienced,
        skill_requirements: &[(Agility, 62), (Endurance, 60)],
        quest_prerequisites: &[8, 14],
        quest_giver: "Chief Miller",
        district: "Industrial Foundry Sector",
        synopsis: "Navigate a melting geothermal core, performing precision wall-kicks over molten slag.",
        steps: &[
            "Evacuate foundry control center",
            "Wall-run over erupting lava vents",
            "Manually override cooling valves",
        ],
        failure_conditions: &["Contact with molten slag"],
        xp_rewards: &[(Agility, 90_000), (Endurance, 85_000)],
        credit_reward: 0,
        item_rewards: &["Magma-Proof Sub-Dermal Weave"],
    },
    QuestDefinition {
        id: 21,
        title: "Sniper's Nest",
        difficulty: Experienced,
        skill_requirements: &[(Ballistics, 65), (Shadow, 60)],
        quest_prerequisites: &[16],
        quest_giver: "Agent Grey (Spire Observation Deck)",
        district: "The Spire Megatowers",
        synopsis: "Eliminate 5 rival syndicate spotters across a 1.2km draw-distance urban vista.",
        steps: &[
            "Set up sniper nest",
            "Account for wind bullet drop at 800m",
            "Eliminate 5 spotters without missing 2 shots",
        ],
        failure_conditions: &["Alerting target spotters"],
        xp_rewards: &[(Ballistics, 105_000), (Shadow, 70_000)],
        credit_reward: 0,
        item_rewards: &["Apex Precision Scope"],
    },
    QuestDefinition {
        id: 22,
        title: "Chrome & Bone",
        difficulty: Experienced,
        skill_requirements: &[(Endurance, 68), (CyberMelee, 65)],
        quest_prerequisites: &[13],
        quest_giver: "Arena Master Titus (Undercity Colosseum)",
        district: "Undercity Slums",
        synopsis: "Defeat the cyber-gladiator champion in an underground cage match with zero equipped weapons.",
        steps: &[
            "Enter ring unarmed",
            "Parallely dodge cyber-bull charges",
            "Defeat Iron Titan using bare fists and counters",
        ],
        failure_conditions: &["Death in cage"],
        xp_rewards: &[(Endurance, 120_000), (CyberMelee, 100_000)],
        credit_reward: 0,
        item_rewards: &["Title: The Unbroken Titan"],
    },
    // ---- Tier 4: Master (levels 70-85) ----
    QuestDefinition {
        id: 23,
        title: "Ascent to Olympus",
        difficulty: Master,
        skill_requirements: &[(Agility, 75), (Netrunning, 70)],
        quest_prerequisites: &[17, 19],
        quest_giver: "Viktor Vance",
        district: "The Spire Megatowers",
        synopsis: "Scale the 800m exterior glass monolith of Nexus Prime headquarters using magnetic climbing hooks.",
        steps: &[
            "Scale glass facade during thunderstorm",
            "Dodge corporate drone lasers",
            "Infiltrate Chairman's penthouse",
        ],
        failure_conditions: &["Fall from tower"],
        xp_rewards: &[(Agility, 180_000), (Netrunning, 150_000)],
        credit_reward: 150_000,
        item_rewards: &[],
    },
    QuestDefinition {
        id: 24,
        title: "Master of the Forge",
        difficulty: Master,
        skill_requirements: &[(Engineering, 78)],
        quest_prerequisites: &[20],
        quest_giver: "Anya Rust",
        district: "Industrial Foundry Sector",
        synopsis: "Reconstruct an ancient pre-collapse quantum fabrication printer from recovered orbital wreckage.",
        steps: &[
            "Gather 4 orbital satellite fragments",
            "Synthesize darknet alloy",
            "Fabricate legendary singularity core",
        ],
        failure_conditions: &["Fabrication explosion"],
        xp_rewards: &[(Engineering, 220_000)],
        credit_reward: 0,
        item_rewards: &["Legendary Blueprint: Zero-Point Singularity Blade"],
    },
    QuestDefinition {
        id: 25,
        title: "Zero Day",
        difficulty: Master,
        skill_requirements: &[(Netrunning, 80)],
        quest_prerequisites: &[19],
        quest_giver: "Ghost_0x",
        district: "The Spire Megatowers",
        synopsis: "Unleash a catastrophic autonomous virus into the megacity military grid, crippling orbital defenses.",
        steps: &[
            "Infiltrate central server hub",
            "Upload apex virus",
            "Defend uplink against elite Black Ops waves for 4 minutes",
        ],
        failure_conditions: &["Uplink destroyed"],
        xp_rewards: &[(Netrunning, 260_000)],
        credit_reward: 0,
        item_rewards: &["Black Ice Cyberdeck"],
    },
    QuestDefinition {
        id: 26,
        title: "The Phantom Syndicate",
        difficulty: Master,
        skill_requirements: &[(Shadow, 82), (StreetCredit, 80)],
        quest_prerequisites: &[18, 21],
        quest_giver: "Silent Mira",
        district: "Kuroshio Commercial Hub",
        synopsis: "Orchestrate the assassination of 4 council oligarchs simultaneously across different districts.",
        steps: &[
            "Plant poison in Spire banquet",
            "Rig foundry limousine",
            "Hack medical pacemaker",
            "Snipe target from crane",
        ],
        failure_conditions: &["Target escapes district"],
        xp_rewards: &[(Shadow, 300_000), (StreetCredit, 250_000)],
        credit_reward: 0,
        item_rewards: &["Title: The Invisible Hand"],
    },
    QuestDefinition {
        id: 27,
        title: "Ballistic Ballet",
        difficulty: Master,
        skill_requirements: &[(Ballistics, 84), (Agility, 80)],
        quest_prerequisites: &[21, 23],
        quest_giver: "Master Jin",
        district: "The Spire Megatowers",
        synopsis: "Clear a 100-enemy combat simulation trial achieving 90%+ headshot accuracy while airborne.",
        steps: &[
            "Enter VR holo-sim",
            "Clear wave 1-50 while wall-running",
            "Clear wave 51-100 with air dashes",
            "Maintain accuracy",
        ],
        failure_conditions: &["Accuracy drops below 90%"],
        xp_rewards: &[(Ballistics, 350_000), (Agility, 280_000)],
        credit_reward: 0,
        item_rewards: &["Exotic Mod: Acrobatic Ricochet"],
    },
    QuestDefinition {
        id: 28,
        title: "Immortal Cyberware",
        difficulty: Master,
        skill_requirements: &[(Endurance, 85)],
        quest_prerequisites: &[22],
        quest_giver: "Dr. Zhao",
        district: "Industrial Foundry Sector",
        synopsis: "Undergo full synthetic body replacement, surviving the psychological shock of disembodiment.",
        steps: &[
            "Enter surgical pod",
            "Withstand neurological fever dreams",
            "Defeat subconscious avatars",
            "Awaken transformed",
        ],
        failure_conditions: &["Mental breakdown"],
        xp_rewards: &[(Endurance, 400_000)],
        credit_reward: 0,
        item_rewards: &["Permanent +25% Max Health"],
    },
    // ---- Tier 5: Grandmaster (levels 85-99) ----
    QuestDefinition {
        id: 29,
        title: "The Fall of Olympus",
        difficulty: Grandmaster,
        skill_requirements: ALL_SKILLS_88,
        quest_prerequisites: &[23, 24, 25, 26, 27, 28],
        quest_giver: "Viktor Vance & Nyx",
        district: "The Spire Megatowers (Apex Citadel)",
        synopsis: "Coordinated multi-district assault to bring down the corporate council's central AI nexus.",
        steps: &[
            "Lead infiltration squad through Spire skybridge",
            "Disable orbital defense cannon",
            "Breach AI core sanctum",
            "Defeat Council Chairman in mechanized exosuit",
            "Format central AI",
        ],
        failure_conditions: &["Total squad wipe"],
        xp_rewards: ALL_SKILLS_750K,
        credit_reward: 1_000_000,
        item_rewards: &["Council Destroyer Cape"],
    },
    QuestDefinition {
        id: 30,
        title: "Protocol 99: Apex Convergence",
        difficulty: Grandmaster,
        skill_requirements: &[(Agility, 92), (CyberMelee, 92)],
        quest_prerequisites: &[29],
        quest_giver: "Mysterious Signal (Dimensional Rift above Spire)",
        district: "The Dimensional Rift",
        synopsis: "The ultimate solo/co-op trial. Battle through the dimensional breach above Neo-Veridia, facing shadow manifestations of your own avatar with randomized loadouts.",
        steps: &[
            "Enter rift anomaly",
            "Survive non-Euclidean parkour obstacle course",
            "Defeat 4 shadow avatars of yourself",
            "Slay the Void Dragon",
            "Absorb the Prime Code",
        ],
        failure_conditions: &["Any fall into void", "Death"],
        xp_rewards: &[(Agility, 1_500_000), (CyberMelee, 1_500_000)],
        credit_reward: 0,
        item_rewards: &["Cosmic Void Cyber-Katana", "Title: Apex God of the Veil"],
    },
];

pub fn find_quest(id: u32) -> Option<&'static QuestDefinition> {
    MASTER_QUEST_CATALOG.iter().find(|q| q.id == id)
}

/// A prerequisite cycle in `catalog`, if any, as the chain of quest ids.
pub fn find_prerequisite_cycle(catalog: &[QuestDefinition]) -> Option<Vec<u32>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Unvisited,
        Visiting,
        Done,
    }
    fn visit(
        catalog: &[QuestDefinition],
        index: usize,
        marks: &mut [Mark],
        path: &mut Vec<u32>,
    ) -> Option<Vec<u32>> {
        match marks[index] {
            Mark::Done => return None,
            Mark::Visiting => {
                let start = path.iter().position(|&id| id == catalog[index].id)?;
                let mut cycle = path[start..].to_vec();
                cycle.push(catalog[index].id);
                return Some(cycle);
            }
            Mark::Unvisited => {}
        }
        marks[index] = Mark::Visiting;
        path.push(catalog[index].id);
        for prerequisite in catalog[index].quest_prerequisites {
            if let Some(next) = catalog.iter().position(|q| q.id == *prerequisite) {
                if let Some(cycle) = visit(catalog, next, marks, path) {
                    return Some(cycle);
                }
            }
        }
        path.pop();
        marks[index] = Mark::Done;
        None
    }
    let mut marks = vec![Mark::Unvisited; catalog.len()];
    (0..catalog.len()).find_map(|i| visit(catalog, i, &mut marks, &mut Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn thirty_quests_with_ids_1_to_30_in_order() {
        assert_eq!(MASTER_QUEST_CATALOG.len(), 30);
        let ids: Vec<u32> = MASTER_QUEST_CATALOG.iter().map(|q| q.id).collect();
        assert_eq!(ids, (1..=30).collect::<Vec<_>>());
        let titles: HashSet<_> = MASTER_QUEST_CATALOG.iter().map(|q| q.title).collect();
        assert_eq!(titles.len(), 30, "unique titles");
    }

    #[test]
    fn tiers_follow_the_campaign_structure() {
        for q in MASTER_QUEST_CATALOG {
            let expected = match q.id {
                1..=6 => Novice,
                7..=14 => Intermediate,
                15..=22 => Experienced,
                23..=28 => Master,
                _ => Grandmaster,
            };
            assert_eq!(q.difficulty, expected, "Q{:02}", q.id);
        }
    }

    #[test]
    fn prerequisites_exist_and_form_a_dag() {
        for q in MASTER_QUEST_CATALOG {
            for p in q.quest_prerequisites {
                assert!(
                    find_quest(*p).is_some(),
                    "Q{:02} requires missing Q{p:02}",
                    q.id
                );
                assert_ne!(*p, q.id, "Q{:02} requires itself", q.id);
            }
        }
        assert_eq!(find_prerequisite_cycle(MASTER_QUEST_CATALOG), None);
    }

    #[test]
    fn cycle_detection_catches_a_loop() {
        let mut looped: Vec<QuestDefinition> = MASTER_QUEST_CATALOG.to_vec();
        looped[0].quest_prerequisites = &[3]; // Q01 -> Q03 -> Q01
        let cycle = find_prerequisite_cycle(&looped).expect("cycle found");
        assert_eq!(cycle.first(), cycle.last());
        assert!(cycle.contains(&1) && cycle.contains(&3), "{cycle:?}");
    }

    #[test]
    fn every_quest_is_fully_specified() {
        for q in MASTER_QUEST_CATALOG {
            assert!(!q.steps.is_empty(), "Q{:02} steps", q.id);
            assert!(!q.failure_conditions.is_empty(), "Q{:02} failure", q.id);
            assert!(!q.xp_rewards.is_empty(), "Q{:02} xp", q.id);
            assert!(!q.quest_giver.is_empty() && !q.district.is_empty() && !q.synopsis.is_empty());
            for &(_, level) in q.skill_requirements {
                assert!(
                    (1..=99).contains(&level),
                    "Q{:02} requirement {level}",
                    q.id
                );
            }
            if q.id > 1 {
                assert!(!q.quest_prerequisites.is_empty(), "only Q01 is a root");
            }
        }
    }

    #[test]
    fn spec_spot_checks() {
        let q1 = find_quest(1).expect("Q01");
        assert_eq!(q1.title, "Protocol Zero");
        assert!(q1.skill_requirements.is_empty() && q1.quest_prerequisites.is_empty());
        assert_eq!(q1.xp_rewards, &[(Agility, 450), (Ballistics, 300)]);
        assert_eq!(
            (q1.credit_reward, q1.item_rewards),
            (1_200, &["Stalker Trenchcoat"][..])
        );

        let q2 = find_quest(2).expect("Q02");
        assert_eq!(
            (q2.skill_requirements, q2.quest_prerequisites),
            (&[(Agility, 5)][..], &[1u32][..])
        );
        assert_eq!(q2.xp_rewards, &[(Agility, 1_200)]);

        assert_eq!(find_quest(7).expect("Q07").quest_prerequisites, &[2, 4]);
        assert_eq!(find_quest(20).expect("Q20").quest_prerequisites, &[8, 14]);

        let q29 = find_quest(29).expect("Q29");
        assert_eq!(q29.skill_requirements.len(), 8);
        assert!(q29.skill_requirements.iter().all(|&(_, l)| l == 88));
        assert_eq!(q29.quest_prerequisites, &[23, 24, 25, 26, 27, 28]);
        assert_eq!(q29.credit_reward, 1_000_000);
        assert_eq!(q29.item_rewards, &["Council Destroyer Cape"]);

        let q30 = find_quest(30).expect("Q30");
        assert_eq!(q30.title, "Protocol 99: Apex Convergence");
        assert_eq!(q30.skill_requirements, &[(Agility, 92), (CyberMelee, 92)]);
        assert_eq!(
            q30.xp_rewards,
            &[(Agility, 1_500_000), (CyberMelee, 1_500_000)]
        );
        assert!(q30.item_rewards.contains(&"Cosmic Void Cyber-Katana"));
        assert!(find_quest(31).is_none());
    }
}
