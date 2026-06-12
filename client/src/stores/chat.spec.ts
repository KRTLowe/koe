import { describe, expect, it } from "vitest";
import { canStartReplyInSession } from "./chat";

describe("chat concurrency", () => {
  it("blocks starting a new reply in another session while one session is responding", () => {
    expect(canStartReplyInSession("b", { activeReplySessionId: "a" })).toBe(false);
  });

  it("allows starting a reply in the same session that is already responding", () => {
    expect(canStartReplyInSession("a", { activeReplySessionId: "a" })).toBe(true);
  });

  it("allows starting a reply when no session is responding", () => {
    expect(canStartReplyInSession("a", { activeReplySessionId: null })).toBe(true);
  });
});
