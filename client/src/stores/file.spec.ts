import { describe, expect, it } from "vitest";
import { normalizeFileHistory } from "./file";

describe("file history", () => {
  it("keeps newest record first for list views", () => {
    const history = normalizeFileHistory([
      { id: "1", name: "a", size: 1, direction: "received" as const, timestamp: 1, status: "ok" as const },
      { id: "2", name: "b", size: 1, direction: "sent" as const, timestamp: 2, status: "ok" as const },
    ]);
    expect(history[0].id).toBe("2");
  });
});
