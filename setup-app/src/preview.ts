import type { EnvironmentInfo, PreparationEvent } from "./types";

const preparationSteps = [
  [18, "Game files verified"],
  [48, "Building save serializers"],
  [64, "Compiling ARM serializer"],
  [82, "Optimizing textures"],
  [96, "Verifying package"],
  [100, "Package ready"],
] as const;

export function previewEnvironment(): EnvironmentInfo {
  return {
    game: {
      path: "/Users/you/Library/Application Support/Steam/steamapps/common/Stardew Valley",
      source: "Steam library",
      supported: true,
      version: "1.6.15.24356",
      detail: "Compatibility build 1.6.15.24356",
    },
    requirements: [
      {
        name: "Mono 6",
        ready: true,
        detail: "Mono JIT compiler version 6.12.0",
      },
      { name: "Mono serializer", ready: true, detail: "sgen is available" },
      { name: "Docker", ready: true, detail: "Docker 28.3.2" },
    ],
    defaultOutput: "/Users/you/Downloads/Stardew Valley for Miyoo Mini",
    releaseKit: {
      ready: true,
      detail: "Included",
    },
    platform: "macos",
  };
}

export async function previewPreparation(
  outputPath: string,
  cancelled: () => boolean,
  emit: (event: PreparationEvent) => void,
) {
  for (const [progress, message] of preparationSteps) {
    await new Promise((resolve) => window.setTimeout(resolve, 350));
    if (cancelled()) {
      emit({ kind: "cancelled", message: "Preparation cancelled", progress: 0 });
      return;
    }
    emit({
      kind: progress === 100 ? "done" : "status",
      message,
      progress,
      outputPath,
    });
  }
}
