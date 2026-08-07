import "@fontsource-variable/nunito-sans";
import "@fontsource-variable/pixelify-sans";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import packageMetadata from "../package.json";
import {
  Check,
  CircleAlert,
  CircleCheck,
  Copy,
  Download,
  FileArchive,
  FolderOpen,
  LoaderCircle,
  PackageCheck,
  Play,
  QrCode,
  RefreshCw,
  Save,
  SquareTerminal,
  X,
  createIcons,
} from "lucide";
import { previewEnvironment, previewPreparation } from "./preview";
import { steamQrMatrix } from "./steam-qr";
import { buildSupportLog } from "./support-log";
import type {
  EnvironmentInfo,
  GameCandidate,
  PreparationEvent,
  Requirement,
  SteamDownloadEvent,
} from "./types";

const icons = {
  Check,
  CircleAlert,
  CircleCheck,
  Copy,
  Download,
  FileArchive,
  FolderOpen,
  LoaderCircle,
  PackageCheck,
  Play,
  QrCode,
  RefreshCw,
  Save,
  SquareTerminal,
  X,
};

const appVersion = packageMetadata.version;
const isTauri = "__TAURI_INTERNALS__" in window;
let environment: EnvironmentInfo | null = null;
let game: GameCandidate | undefined;
let outputPath = "";
let preparing = false;
let steamDownloading = false;
let previewCancelled = false;
let logLines: string[] = [];
let steamLogLines: string[] = [];
let steamQrLines: string[] = [];
let logStatusTimer: number | undefined;

type Stage = "game" | "system" | "prepare";

const stages: Stage[] = ["game", "system", "prepare"];

const element = <T extends HTMLElement>(id: string): T => {
  const found = document.getElementById(id);
  if (!found) throw new Error(`Missing element: ${id}`);
  return found as T;
};

function refreshIcons() {
  createIcons({ icons });
}

async function loadEnvironment() {
  setOverall("checking", "Checking this computer");
  element<HTMLButtonElement>("refresh").disabled = true;
  try {
    environment = isTauri
      ? await invoke<EnvironmentInfo>("inspect_environment")
      : previewEnvironment();
    game = environment.game;
    if (!outputPath) outputPath = environment.defaultOutput;
    render();
  } catch (error) {
    setOverall("error", readableError(error));
  } finally {
    element<HTMLButtonElement>("refresh").disabled = false;
    refreshIcons();
  }
}

function render() {
  if (!environment) return;
  renderGame();
  renderRequirements();
  element("output-path").textContent = outputPath;
  element("output-path").title = outputPath;

  const requirementsReady = environment.requirements.every(
    (item) => item.ready,
  );
  const ready = Boolean(
    game?.supported && environment.releaseKit.ready && requirementsReady,
  );
  element<HTMLButtonElement>("prepare").disabled = preparing
    ? false
    : !ready || steamDownloading;
  element<HTMLButtonElement>("download-steam").disabled = preparing;
  element<HTMLButtonElement>("choose-game").disabled =
    preparing || steamDownloading;
  element<HTMLButtonElement>("choose-zip").disabled =
    preparing || steamDownloading;
  element<HTMLButtonElement>("choose-output").disabled =
    preparing || steamDownloading;
  renderSteamButton();
  renderPrepareButton();

  if (ready) {
    setOverall("ready", "Ready to prepare");
    setStageState("prepare", ["game", "system"]);
  } else {
    let message = "Release files missing";
    if (!game?.supported) message = "Game files needed";
    else if (!requirementsReady) message = "Required software missing";
    setOverall("error", message);
    setStageState(
      game?.supported ? "system" : "game",
      game?.supported ? ["game"] : [],
    );
  }
  renderSupportLog();
  refreshIcons();
}

function renderGame() {
  const state = element("game-state");
  if (!game) {
    element("game-path").textContent = "Stardew Valley was not found in Steam";
    element("game-path").title = "Stardew Valley was not found in Steam";
    element("game-detail").textContent =
      "Sign in with Steam or choose existing game files.";
    state.textContent = "Missing";
    state.className = "state-label error";
    return;
  }
  element("game-path").textContent = game.path;
  element("game-path").title = game.path;
  element("game-detail").textContent = game.detail;
  state.textContent = game.supported ? "Supported" : "Not supported";
  state.className = `state-label ${game.supported ? "ready" : "error"}`;
}

function renderRequirements() {
  if (!environment) return;
  const requirements = [
    ...environment.requirements,
    {
      name: "Release files",
      ready: environment.releaseKit.ready,
      detail: environment.releaseKit.detail,
      helpUrl: undefined,
    },
  ];
  element("requirements").replaceChildren(
    ...requirements.map(renderRequirement),
  );
}

function renderRequirement(item: Requirement): HTMLElement {
  const row = document.createElement("div");
  row.className = `requirement ${item.ready ? "ready" : "error"}`;

  const icon = document.createElement("i");
  icon.dataset.lucide = item.ready ? "circle-check" : "circle-alert";
  icon.setAttribute("aria-hidden", "true");

  const copy = document.createElement("div");
  const name = document.createElement("strong");
  const detail = document.createElement("small");
  name.textContent = item.name;
  detail.textContent = item.detail;
  copy.append(name, detail);
  row.append(icon, copy);

  const helpUrl = item.helpUrl;
  if (!item.ready && helpUrl) {
    const guide = document.createElement("button");
    guide.className = "requirement-action";
    guide.type = "button";
    guide.textContent = "Open guide";
    guide.addEventListener("click", () =>
      runAction(() => openRequirementSetup(helpUrl)),
    );
    row.append(guide);
  }
  return row;
}

async function openRequirementSetup(url: string) {
  if (isTauri) await openUrl(url);
  else window.open(url, "_blank", "noopener,noreferrer");
}

function setOverall(kind: "checking" | "ready" | "error", text: string) {
  const status = element("overall-status");
  const iconName = {
    checking: "loader-circle",
    ready: "circle-check",
    error: "circle-alert",
  }[kind];
  const icon = document.createElement("i");
  const label = document.createElement("span");
  icon.dataset.lucide = iconName;
  icon.setAttribute("aria-hidden", "true");
  label.textContent = text;
  status.className = `overall-status ${kind}`;
  status.replaceChildren(icon, label);
  refreshIcons();
}

function setStageState(active: Stage | undefined, completed: Stage[]) {
  const completedStages = new Set(completed);
  document.querySelectorAll<HTMLElement>(".stage-list li").forEach((item) => {
    const stage = item.dataset.stage as Stage;
    const isActive = stage === active;
    const isComplete = completedStages.has(stage);
    const marker = item.querySelector<HTMLElement>(".stage-marker");

    item.classList.toggle("active", isActive);
    item.classList.toggle("complete", isComplete);
    if (isActive) item.setAttribute("aria-current", "step");
    else item.removeAttribute("aria-current");

    if (!marker) return;
    if (isComplete) {
      const icon = document.createElement("i");
      icon.dataset.lucide = "check";
      icon.setAttribute("aria-hidden", "true");
      marker.replaceChildren(icon);
    } else {
      marker.textContent = String(stages.indexOf(stage) + 1);
    }
  });
  refreshIcons();
}

async function chooseGameFolder() {
  if (!isTauri) return;
  const selected = await open({
    directory: true,
    multiple: false,
    defaultPath: game?.path,
  });
  if (!selected) return;
  game = await invoke<GameCandidate>("inspect_game", { path: selected });
  render();
}

async function chooseGameZip() {
  if (!isTauri) return;
  const selected = await open({
    multiple: false,
    filters: [{ name: "Stardew Valley ZIP", extensions: ["zip"] }],
  });
  if (!selected) return;
  setOverall("checking", "Checking ZIP archive");
  game = await invoke<GameCandidate>("inspect_game", { path: selected });
  render();
}

async function chooseOutput() {
  if (!isTauri) return;
  const selected = await open({
    directory: true,
    multiple: false,
    defaultPath: outputPath,
  });
  if (!selected) return;
  outputPath = selected;
  render();
}

async function downloadFromSteam() {
  if (!isTauri || preparing) return;
  if (steamDownloading) {
    await cancelSteamDownload();
    return;
  }
  steamDownloading = true;
  steamLogLines = [];
  steamQrLines = [];
  element("steam-panel").hidden = false;
  element("steam-title").textContent = "Connecting to Steam";
  element("steam-message").textContent = "Starting Steam Mobile sign-in";
  element("steam-qr").hidden = true;
  element("steam-output").hidden = false;
  element("steam-output").textContent = "Starting...";
  updateSteamProgress(1);
  setOverall("checking", "Connecting to Steam");
  renderSteamButton();
  try {
    await invoke("start_steam_download");
  } catch (error) {
    finishSteamWithError(readableError(error));
  }
}

async function cancelSteamDownload() {
  element("steam-message").textContent = "Stopping the Steam download";
  try {
    await invoke("cancel_steam_download");
  } catch (error) {
    appendSteamLog(`Could not stop download: ${readableError(error)}`);
  }
}

function handleSteamProgress(event: SteamDownloadEvent) {
  updateSteamProgress(event.progress);
  if (event.kind === "qr-start") {
    appendSteamLog(event.message);
    showSteamQr();
    element("steam-title").textContent = "Sign in with Steam Mobile";
    element("steam-message").textContent =
      "Open Steam Mobile, scan the code and approve the sign-in";
    return;
  }
  if (event.kind === "qr") {
    appendSteamQrLine(event.message);
    return;
  }

  appendSteamLog(event.message);
  if (event.progress > 16) hideSteamQr();
  if (event.kind === "status") {
    element("steam-message").textContent = event.message;
  } else if (event.kind === "error") {
    finishSteamWithError(event.message);
  } else if (event.kind === "cancelled") {
    steamDownloading = false;
    hideSteamQr();
    element("steam-title").textContent = "Steam download cancelled";
    element("steam-message").textContent = "Press Steam to try again";
    setOverall("error", "Game files needed");
    renderSteamButton();
  } else if (event.kind === "done" && event.game) {
    steamDownloading = false;
    hideSteamQr();
    game = event.game;
    if (environment) environment.game = event.game;
    element("steam-title").textContent = "Steam download complete";
    element("steam-message").textContent = event.game.detail;
    renderSteamButton();
    render();
  }
}

function finishSteamWithError(message: string) {
  steamDownloading = false;
  hideSteamQr();
  appendSteamLog(`Error: ${message}`);
  element("steam-title").textContent = "Steam download stopped";
  element("steam-message").textContent = message;
  setOverall("error", "Steam download stopped");
  renderSteamButton();
}

function updateSteamProgress(progress: number) {
  element("steam-progress-percent").textContent = `${progress}%`;
  element<HTMLElement>("steam-progress-bar").style.width = `${progress}%`;
}

function appendSteamLog(line: string) {
  if (!line.trim()) return;
  if (
    steamLogLines.length === 0 &&
    element("steam-output").textContent === "Starting..."
  ) {
    element("steam-output").textContent = "";
  }
  steamLogLines.push(line);
  if (steamLogLines.length > 500) steamLogLines = steamLogLines.slice(-500);
  element("steam-output").textContent = steamLogLines.join("\n");
  renderSupportLog();
}

function showSteamQr() {
  steamQrLines = [];
  const qr = element("steam-qr");
  qr.replaceChildren();
  qr.hidden = false;
  element("steam-output").hidden = true;
}

function appendSteamQrLine(line: string) {
  steamQrLines.push(line);
  renderSteamQr();
}

function renderSteamQr() {
  const rows = steamQrMatrix(steamQrLines);
  const columns = Math.max(0, ...rows.map((row) => row.length));
  if (!rows.length || !columns) return;

  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", `0 0 ${columns} ${rows.length}`);
  svg.setAttribute("aria-hidden", "true");
  for (const [y, row] of rows.entries()) {
    for (const [x, dark] of row.entries()) {
      if (!dark) continue;
      const module = document.createElementNS(
        "http://www.w3.org/2000/svg",
        "rect",
      );
      module.setAttribute("x", String(x));
      module.setAttribute("y", String(y));
      module.setAttribute("width", "1");
      module.setAttribute("height", "1");
      svg.append(module);
    }
  }
  element("steam-qr").replaceChildren(svg);
}

function hideSteamQr() {
  element("steam-qr").hidden = true;
  element("steam-output").hidden = false;
}

function renderSteamButton() {
  const button = element<HTMLButtonElement>("download-steam");
  button.classList.toggle("cancel-button", steamDownloading);
  button.disabled = preparing;
  button.innerHTML = steamDownloading
    ? '<i data-lucide="x" aria-hidden="true"></i>Cancel'
    : '<i data-lucide="download" aria-hidden="true"></i>Steam';
  refreshIcons();
}

async function prepare() {
  if (preparing) {
    await cancelPreparation();
    return;
  }
  if (!game?.supported || !outputPath) return;
  preparing = true;
  previewCancelled = false;
  logLines = [];
  element("result-section").hidden = true;
  element("progress-section").hidden = false;
  element("progress-title").textContent = "Preparing files";
  setOverall("checking", "Preparing package");
  setStageState("prepare", ["game", "system"]);
  renderPrepareButton();
  updateProgress(2, "Validating the complete game folder");
  appendLog("Validating game files...");
  try {
    const result = isTauri
      ? await invoke<string>("validate_game", { path: game.path })
      : "Game files verified";
    appendLog(result);
    if (isTauri) {
      await invoke("start_preparation", { gamePath: game.path, outputPath });
    } else {
      await previewPreparation(
        outputPath,
        () => previewCancelled,
        handleProgress,
      );
    }
  } catch (error) {
    finishWithError(readableError(error));
  }
}

async function cancelPreparation() {
  updateProgress(
    Number.parseInt(element("progress-percent").textContent || "0", 10),
    "Stopping preparation",
  );
  appendLog("Stopping preparation...");
  if (!isTauri) {
    previewCancelled = true;
    return;
  }
  try {
    await invoke("cancel_preparation");
  } catch (error) {
    appendLog(`Could not stop preparation: ${readableError(error)}`);
  }
}

function handleProgress(event: PreparationEvent) {
  if (event.kind === "error") {
    finishWithError(event.message);
    return;
  }
  if (event.kind === "cancelled") {
    preparing = false;
    appendLog(event.message);
    updateProgress(0, event.message);
    element("progress-title").textContent = "Preparation cancelled";
    setOverall("ready", "Ready to prepare");
    renderPrepareButton();
    return;
  }
  appendLog(event.message);
  updateProgress(event.progress, event.message);
  if (event.kind === "done") {
    preparing = false;
    if (event.outputPath) outputPath = event.outputPath;
    element("progress-title").textContent = "Preparation complete";
    element("result-section").hidden = false;
    setOverall("ready", "Package ready");
    setStageState(undefined, ["game", "system", "prepare"]);
    renderPrepareButton();
    refreshIcons();
  }
}

function finishWithError(message: string) {
  preparing = false;
  appendLog(`Error: ${message}`);
  updateProgress(0, message);
  element("progress-title").textContent = "Preparation stopped";
  setOverall("error", "Preparation stopped");
  renderPrepareButton();
}

function renderPrepareButton() {
  const button = element<HTMLButtonElement>("prepare");
  button.classList.toggle("cancel-button", preparing);
  button.innerHTML = preparing
    ? '<i data-lucide="x" aria-hidden="true"></i>Cancel'
    : '<i data-lucide="play" aria-hidden="true"></i>Prepare package';
  if (preparing) button.disabled = false;
  refreshIcons();
}

function updateProgress(progress: number, message: string) {
  element("progress-percent").textContent = `${progress}%`;
  element<HTMLElement>("progress-bar").style.width = `${progress}%`;
  element("progress-message").textContent = message;
}

function appendLog(line: string) {
  if (!line.trim()) return;
  logLines.push(line);
  if (logLines.length > 400) logLines = logLines.slice(-400);
  renderSupportLog();
}

function toggleLog(visible?: boolean) {
  const panel = element("log-panel");
  panel.hidden = visible === undefined ? !panel.hidden : !visible;
  element("toggle-log").querySelector("span")!.textContent = panel.hidden
    ? "Show log"
    : "Hide log";
  if (!panel.hidden) {
    renderSupportLog();
    window.requestAnimationFrame(() => panel.scrollIntoView({ block: "nearest" }));
  }
}

function supportLog(): string {
  const requirements = environment
    ? [
        ...environment.requirements,
        {
          name: "Release files",
          ready: environment.releaseKit.ready,
          detail: environment.releaseKit.detail,
        },
      ]
    : [];
  return buildSupportLog({
    appVersion,
    platform: environment?.platform,
    game,
    outputPath,
    requirements,
    steamLines: steamLogLines,
    preparationLines: logLines,
  });
}

function renderSupportLog() {
  element("log-output").textContent = supportLog();
}

async function copyLog() {
  try {
    const contents = supportLog();
    if (isTauri) await writeText(contents);
    else await copyInBrowser(contents);
    setLogActionStatus("Copied to clipboard");
  } catch (error) {
    setLogActionStatus(`Could not copy: ${readableError(error)}`, true);
  }
}

async function saveLog() {
  try {
    const contents = supportLog();
    if (isTauri) {
      const path = await saveDialog({
        defaultPath: "stardew-miyoo-setup-log.txt",
        filters: [{ name: "Text file", extensions: ["txt"] }],
      });
      if (!path) return;
      await invoke("save_log", { path, contents });
    } else {
      downloadInBrowser(contents);
    }
    setLogActionStatus("Log saved");
  } catch (error) {
    setLogActionStatus(`Could not save: ${readableError(error)}`, true);
  }
}

async function copyInBrowser(contents: string) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(contents);
    return;
  }
  const input = document.createElement("textarea");
  input.value = contents;
  input.style.position = "fixed";
  input.style.opacity = "0";
  document.body.append(input);
  input.select();
  const copied = document.execCommand("copy");
  input.remove();
  if (!copied) throw new Error("clipboard access was denied");
}

function downloadInBrowser(contents: string) {
  const url = URL.createObjectURL(
    new Blob([contents], { type: "text/plain;charset=utf-8" }),
  );
  const link = document.createElement("a");
  link.href = url;
  link.download = "stardew-miyoo-setup-log.txt";
  link.click();
  URL.revokeObjectURL(url);
}

function setLogActionStatus(message: string, error = false) {
  const status = element("log-action-status");
  window.clearTimeout(logStatusTimer);
  status.textContent = message;
  status.classList.toggle("error", error);
  logStatusTimer = window.setTimeout(() => {
    status.textContent = "";
    status.classList.remove("error");
  }, 4000);
}

function readableError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function runAction(action: () => Promise<unknown>) {
  void action().catch((error) => setOverall("error", readableError(error)));
}

function bindAction(id: string, action: () => Promise<unknown>) {
  element<HTMLButtonElement>(id).addEventListener("click", () =>
    runAction(action),
  );
}

async function start() {
  refreshIcons();
  element("app-version").textContent = `Version ${appVersion}`;
  bindAction("choose-game", chooseGameFolder);
  bindAction("choose-zip", chooseGameZip);
  bindAction("download-steam", downloadFromSteam);
  bindAction("choose-output", chooseOutput);
  bindAction("refresh", loadEnvironment);
  bindAction("prepare", prepare);
  element("toggle-log").addEventListener("click", () => toggleLog());
  element("close-log").addEventListener("click", () => toggleLog(false));
  bindAction("copy-log", copyLog);
  bindAction("save-log", saveLog);
  bindAction("open-output", async () => {
    if (isTauri) await revealItemInDir(outputPath);
  });
  if (isTauri) {
    await listen<PreparationEvent>("preparation-progress", ({ payload }) =>
      handleProgress(payload),
    );
    await listen<SteamDownloadEvent>("steam-download-progress", ({ payload }) =>
      handleSteamProgress(payload),
    );
  }
  await loadEnvironment();
}

window.addEventListener("DOMContentLoaded", () => runAction(start));
