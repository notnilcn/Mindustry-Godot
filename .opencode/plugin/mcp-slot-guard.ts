// SPDX-License-Identifier: GPL-3.0-only
/**
 * MCP slot guard — enforces `MCP_SLOT_LIMIT` concurrent MCP consumers per
 * host. The default is 0 (uncapped): every caller gets a lease. Set a positive
 * value for the parity plan's laptop-safety budget (2 was the two slots the
 * loops and their fix/twin legs shared).
 *
 * Every opencode process takes one lease on its first MCP tool call and
 * refreshes it on later calls (`.opencode/skills/parity-eval/scripts/mcp_slot.py`).
 * When the registry is full, workflow agents listed in `MCP_SLOT_GUARD_AGENTS`
 * (default `gap-identifier,parity-orchestrator,parity-writer,twin-evaluator`)
 * get their call denied;
 * everyone else is warned and allowed, because the guard must never deadlock a
 * human's session. `parity-writer` is additionally denied every
 * `computer-mcp_*` tool:
 * its Godot leg uses open-godot-mcp, and the Java reference belongs to the twin
 * evaluator. `MCP_SLOT_GUARD=off` disables the guard entirely.
 *
 * Failure policy: acquire errors other than "limit" fail open with a console
 * warning; a refresh or release whose lease an agent already released (exit 4)
 * is silent and falls through to acquire, so explicit releases cannot leave one
 * call unaccounted or spam the console. A process
 * that has made no MCP call for `QUIET_MS` releases its lease so waiting loops
 * can take the slot; its next call re-acquires. The script path is resolved
 * from the project directory, so loop worktrees share the main checkout's
 * registry through the git common dir.
 *
 * Loaded once at opencode start; changes here need an opencode restart.
 */
import type { Plugin } from "@opencode-ai/plugin"
import { execFileSync } from "node:child_process"
import { join } from "node:path"

const MCP_TOOL = /^(open-godot-mcp|computer-mcp)_/
const SCRIPT = ".opencode/skills/parity-eval/scripts/mcp_slot.py"
const OWNER = `oc-${process.pid}`
const QUIET_MS = 60_000
const GATED = (
  process.env.MCP_SLOT_GUARD_AGENTS ??
  "gap-identifier,parity-orchestrator,parity-writer,twin-evaluator"
)
  .split(",")
  .map((name) => name.trim())
  .filter(Boolean)

// Tools that specific agents may never call, regardless of slot state.
const DENIED: Record<string, RegExp> = {
  "parity-writer": /^computer-mcp_/,
}

type AcquireResult = { ok: true; degraded?: boolean } | { ok: false }

export default (async ({ directory }) => {
  if (process.env.MCP_SLOT_GUARD === "off") return {}

  const sessionAgent = new Map<string, string>()
  let token: string | null = null
  let lastUse = 0
  let quietTimer: ReturnType<typeof setTimeout> | null = null

  const run = (args: string[]) =>
    execFileSync("python3", [join(directory, SCRIPT), ...args], {
      cwd: directory,
      encoding: "utf8",
      env: { ...process.env, MCP_SLOT_OWNER_KEY: OWNER },
      stdio: ["ignore", "pipe", "pipe"],
    })

  const acquire = (): AcquireResult => {
    try {
      const parsed = JSON.parse(run(["acquire", "--owner", OWNER]))
      token = typeof parsed.token === "string" ? parsed.token : null
      return { ok: true }
    } catch (error) {
      const status = (error as { status?: number }).status
      if (status === 3) return { ok: false }
      console.error(
        `[mcp-slot-guard] acquire failed, allowing MCP (fail-open): ${String(error)}`,
      )
      return { ok: true, degraded: true }
    }
  }

  const refresh = (): boolean => {
    if (!token) return false
    try {
      run(["refresh", "--token", token])
      return true
    } catch (error) {
      // Exit 4 is unknown-token: an agent released the lease explicitly (the
      // end-of-stage contract); acquire() below takes a new one.
      if ((error as { status?: number }).status !== 4) {
        console.error(`[mcp-slot-guard] refresh failed: ${String(error)}`)
      }
      token = null
      return false
    }
  }

  const releaseQuietly = () => {
    if (!token) return
    try {
      // By key: also cleans up a lease an agent released or replaced.
      run(["release", "--key", OWNER])
    } catch (error) {
      // Exit 4 is unknown-key: an agent already released the lease, which is
      // the normal case this cleanup exists for, so it is not an error.
      if ((error as { status?: number }).status !== 4) {
        console.error(`[mcp-slot-guard] release failed: ${String(error)}`)
      }
    }
    token = null
  }

  const scheduleQuietRelease = () => {
    if (quietTimer || !token) return
    quietTimer = setTimeout(() => {
      quietTimer = null
      if (!token) return
      if (Date.now() - lastUse < QUIET_MS) {
        scheduleQuietRelease()
        return
      }
      releaseQuietly()
    }, QUIET_MS)
    quietTimer.unref?.()
  }

  process.once("exit", releaseQuietly)

  return {
    "chat.params": async (input) => {
      sessionAgent.set(input.sessionID, input.agent)
    },
    "shell.env": async (_input, output) => {
      // Tool shells share this process's lease key, so an agent's manual
      // `mcp_slot.py acquire` reuses the process lease instead of adding a
      // second one.
      output.env.MCP_SLOT_OWNER_KEY = OWNER
    },
    "tool.execute.before": async (input) => {
      const agent = sessionAgent.get(input.sessionID) ?? ""
      const denied = DENIED[agent]
      if (denied?.test(input.tool)) {
        throw new Error(
          `[mcp-slot-guard] ${agent} may only use open-godot-mcp; ${input.tool} is denied`,
        )
      }
      if (!MCP_TOOL.test(input.tool)) return
      lastUse = Date.now()
      if (token && refresh()) return
      const result = acquire()
      if (result.ok) return
      const message =
        `[mcp-slot-guard] MCP slot limit reached (MCP_SLOT_LIMIT); ` +
        `${GATED.join(", ")} must stay off MCP until a slot frees or a lease expires`
      if (GATED.includes(agent)) throw new Error(message)
      console.error(`${message}; allowing non-gated agent "${agent || "unknown"}"`)
    },
    event: async ({ event }) => {
      if (event.type === "session.idle" || event.type === "session.deleted") {
        scheduleQuietRelease()
      }
      if (event.type === "session.deleted") {
        const id = (event.properties as { info?: { id?: string } })?.info?.id
        if (id) sessionAgent.delete(id)
      }
    },
  }
}) satisfies Plugin
