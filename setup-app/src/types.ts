export type Requirement = {
  name: string;
  ready: boolean;
  detail: string;
  helpUrl?: string;
};

export type GameCandidate = {
  path: string;
  source: string;
  supported: boolean;
  version?: string;
  detail: string;
};

export type EnvironmentInfo = {
  game?: GameCandidate;
  requirements: Requirement[];
  defaultOutput: string;
  releaseKit: {
    ready: boolean;
    detail: string;
  };
  platform: string;
};

export type PreparationEvent = {
  kind: "status" | "log" | "warning" | "done" | "cancelled" | "error";
  message: string;
  progress: number;
  outputPath?: string;
};

export type SteamDownloadEvent = {
  kind:
    | "status"
    | "log"
    | "qr-start"
    | "qr"
    | "warning"
    | "done"
    | "cancelled"
    | "error";
  message: string;
  progress: number;
  game?: GameCandidate;
};
