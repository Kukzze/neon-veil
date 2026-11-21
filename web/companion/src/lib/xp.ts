// Experience curve, identical to crates/game_rpg/src/xp_table.rs.

export const MAX_LEVEL = 99;
export const XP_CAP = 200_000_000;

export const SKILLS = [
  "Agility",
  "Cyber-Melee",
  "Ballistics",
  "Netrunning",
  "Engineering",
  "Shadow",
  "Endurance",
  "Street Credit",
] as const;

export type SkillName = (typeof SKILLS)[number];
export const MAX_TOTAL_LEVEL = SKILLS.length * MAX_LEVEL;

/** XP needed for each level; index = level (index 0 unused). */
export const XP_TABLE: readonly number[] = (() => {
  const table = [0, 0];
  let points = 0;
  for (let level = 1; level < MAX_LEVEL; level++) {
    points += Math.floor(level + 300 * Math.pow(2, level / 7));
    table.push(Math.floor(points / 4));
  }
  return table;
})();

export function levelForXp(xp: number): number {
  let level = 1;
  while (level < MAX_LEVEL && xp >= (XP_TABLE[level + 1] ?? Infinity)) level++;
  return level;
}

export function totalLevel(xp: readonly number[]): number {
  return xp.reduce((sum, value) => sum + levelForXp(value), 0);
}

export function totalXp(xp: readonly number[]): number {
  return xp.reduce((sum, value) => sum + Math.min(value, XP_CAP), 0);
}
