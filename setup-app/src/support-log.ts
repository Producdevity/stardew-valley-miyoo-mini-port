import type { GameCandidate, Requirement } from "./types";

export type SupportLogData = {
  appVersion: string;
  platform?: string;
  game?: GameCandidate;
  outputPath?: string;
  requirements: Requirement[];
  steamLines: string[];
  preparationLines: string[];
};

export function buildSupportLog(data: SupportLogData): string {
  const game = data.game;
  const lines = [
    `Stardew Valley for Miyoo Mini setup ${data.appVersion}`,
    `Generated: ${new Date().toISOString()}`,
    `Platform: ${data.platform || "unknown"}`,
    `Game: ${game ? game.detail : "not found"}`,
    `Game version: ${game?.version || "unknown"}`,
    `Game source: ${game?.source || "unknown"}`,
    `Game path: ${game?.path || "not selected"}`,
    `Game supported: ${game ? String(game.supported) : "false"}`,
    `Package folder: ${data.outputPath || "not selected"}`,
    "",
    "Requirements",
    ...data.requirements.map(
      (item) =>
        `- ${item.name}: ${item.ready ? "ready" : "missing"} (${item.detail})`,
    ),
  ];

  appendSection(lines, "Steam download", data.steamLines);
  appendSection(lines, "Preparation", data.preparationLines);
  return `${lines.join("\n")}\n`;
}

function appendSection(lines: string[], heading: string, entries: string[]) {
  lines.push("", heading, ...(entries.length > 0 ? entries : ["No entries."]));
}
