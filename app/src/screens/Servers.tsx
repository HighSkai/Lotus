import { useRef, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Activity, Check, ClipboardPaste, Copy, FileText, LoaderCircle, Pencil, Plus, Server, Trash2, TriangleAlert, WandSparkles } from "lucide-react";
import { ModeMark } from "@/components/ShadowMark";
import { Hint, IconButton, List, NavRow, Page, Pill } from "@/components/Controls";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { api, errorText } from "@/lib/api";
import { hostLabel } from "@/lib/format";
import { spring } from "@/lib/motion";
import { setState, toast, useStore } from "@/lib/store";
import { cn } from "@/lib/utils";
import type { ImportResult, ServerProfile, TestResult } from "@/lib/types";
import { labelFor } from "./Home";
import type { Nav } from "../App";

export function Servers({ nav }: { nav: Nav }) {
  const profiles = useStore((s) => s.profiles);
  const settings = useStore((s) => s.settings)!;
  const mode = settings.mode;
  const [tests, setTests] = useState<Record<string, TestResult | "running">>({});
  const [doomed, setDoomed] = useState<ServerProfile | null>(null);

  async function test(id: string) {
    setTests((t) => ({ ...t, [id]: "running" }));
    try {
      const r = await api.testProfile(id);
      setTests((t) => ({ ...t, [id]: r }));
      if (!r.ok) toast(r.message, "error");
    } catch (e) {
      setTests((t) => {
        const n = { ...t };
        delete n[id];
        return n;
      });
      toast(errorText(e), "error");
    }
  }

  async function remove(id: string) {
    try {
      setState({ profiles: await api.deleteProfile(id) });
    } catch (e) {
      toast(errorText(e), "error");
    }
  }

  const select = (id: string) => api.selectProfile(id).catch((e) => toast(errorText(e), "error"));

  return (
    <Page
      title="Servers"
      onBack={nav.back}
      actions={
        profiles.length > 0 && (
          <Button size="sm" onClick={() => nav.push({ name: "add-server" })}>
            <Plus /> Add
          </Button>
        )
      }
    >
      {profiles.length === 0 ? (
        <Card className="items-center gap-2 px-5 py-7 text-center">
          <ModeMark mode={mode} className="mb-1 size-14" water />
          <div className="text-[15px] font-semibold">Bring your own server</div>
          <div className="max-w-[260px] text-[12.5px] text-muted-foreground">Websites see your server's address instead of yours.</div>
          <div className="mt-3 flex flex-wrap justify-center gap-2">
            <Button onClick={() => nav.push({ name: "add-server" })}>
              <Plus /> Add a server
            </Button>
            <Button variant="secondary" onClick={() => nav.push({ name: "setup" })}>
              <WandSparkles /> Set one up
            </Button>
          </div>
        </Card>
      ) : (
        <Card className="p-1" role="radiogroup" aria-label="Exit server">
          <AnimatePresence initial={false}>
            {profiles.map((p) => {
              const t = tests[p.id];
              const on = settings.selected_profile === p.id;
              return (
                <motion.div
                  key={p.id}
                  initial={{ opacity: 0, height: 0 }}
                  animate={{ opacity: 1, height: "auto" }}
                  exit={{ opacity: 0, height: 0 }}
                  transition={spring}
                  className="group/server overflow-hidden"
                >
                  <div className={cn("relative flex items-center gap-1 rounded-[18px] py-1 pr-1.5 transition-colors duration-200", on && "bg-primary/[0.07] ring-1 ring-primary/20 ring-inset")}>
                    <button type="button" role="radio" aria-checked={on} data-spot onClick={() => select(p.id)} className="flex min-w-0 flex-1 items-center gap-3 rounded-[16px] py-1.5 pl-3 text-left">
                      <span className="grid size-5 shrink-0 place-items-center">
                        {on ? <Check className="size-[18px] text-primary" strokeWidth={2.4} /> : <Server className="size-[17px] text-muted-foreground" strokeWidth={1.8} />}
                      </span>
                      <span className="flex min-w-0 flex-1 flex-col">
                        <span className="truncate text-[13.5px] font-semibold">{p.name}</span>
                        <span className="truncate text-[11.5px] text-faint">
                          {labelFor(p.protocol.type)} · {hostLabel(p.protocol.server, p.protocol.port)}
                        </span>
                      </span>
                      {t === "running" ? (
                        <LoaderCircle className="size-4 animate-spin text-primary" />
                      ) : t ? (
                        <Pill tone={t.ok ? "good" : "bad"}>{t.ok ? `${t.latency_ms} ms` : "Failed"}</Pill>
                      ) : null}
                    </button>
                    <div className="flex opacity-55 transition-opacity duration-150 group-focus-within/server:opacity-100 group-hover/server:opacity-100">
                      <IconButton label="Test" onClick={() => test(p.id)}>
                        <Activity />
                      </IconButton>
                      <IconButton label="Copy share link" onClick={() => api.copyShareLink(p.id).then(() => toast("Link copied. It contains your server's secret"))}>
                        <Copy />
                      </IconButton>
                      <IconButton label="Edit" onClick={() => nav.push({ name: "edit-server", id: p.id })}>
                        <Pencil />
                      </IconButton>
                      <IconButton label="Delete" onClick={() => setDoomed(p)} className="hover:text-destructive">
                        <Trash2 />
                      </IconButton>
                    </div>
                  </div>
                </motion.div>
              );
            })}
          </AnimatePresence>
        </Card>
      )}

      <AlertDialog open={!!doomed} onOpenChange={(o) => !o && setDoomed(null)}>
        <AlertDialogContent size="sm">
          <AlertDialogHeader>
            <AlertDialogTitle>Delete “{doomed?.name}”?</AlertDialogTitle>
            <AlertDialogDescription>Its keys are removed from this PC.</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={() => doomed && remove(doomed.id)}>
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Page>
  );
}

export function AddServer({ nav }: { nav: Nav }) {
  const [text, setText] = useState("");
  const [result, setResult] = useState<ImportResult | null>(null);
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  async function run(p: Promise<ImportResult>) {
    setBusy(true);
    try {
      const r = await p;
      setResult(r);
      setState({ profiles: r.profiles });
      if (r.added.length) {
        toast(r.added.length === 1 ? `Added “${r.added[0].name}”` : `Added ${r.added.length} servers`);
        if (!r.issues.length) nav.replace({ name: "servers" });
      }
    } catch (e) {
      toast(errorText(e), "error");
    } finally {
      setBusy(false);
    }
  }

  return (
    <Page title="Add server" onBack={nav.back}>
      <Card className="gap-3 p-4">
        <Label htmlFor="imp" className="text-[12px] font-medium text-muted-foreground">
          Paste a link or configuration
        </Label>
        <Textarea
          id="imp"
          rows={5}
          spellCheck={false}
          className="min-h-[112px] font-mono text-[12px] leading-relaxed"
          placeholder={"lotus://  vless://  ss://  trojan://  hysteria2://\nWireGuard .conf or sing-box JSON"}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <div className="flex flex-wrap gap-2">
          <Button disabled={busy || !text.trim()} onClick={() => run(api.importText(text))}>
            {busy ? <LoaderCircle className="animate-spin" /> : <Check />} Import
          </Button>
          <Button variant="secondary" disabled={busy} onClick={() => run(api.importClipboard())}>
            <ClipboardPaste /> Paste
          </Button>
          <Button variant="secondary" disabled={busy} onClick={() => fileRef.current?.click()}>
            <FileText /> File…
          </Button>
          <input
            ref={fileRef}
            type="file"
            accept=".conf,.json,.txt,.yaml,.yml"
            hidden
            onChange={async (e) => {
              const f = e.target.files?.[0];
              e.target.value = "";
              if (!f) return;
              if (f.size > 2 * 1024 * 1024) {
                toast("That file is too large to be a server configuration.", "error");
                return;
              }
              run(api.importText(await f.text()));
            }}
          />
        </div>
      </Card>
      {result && result.issues.length > 0 && (
        <Card className="p-1">
          {result.issues.map((i, n) => (
            <div key={n} className="relative flex gap-3 px-3 py-3 not-first:before:absolute not-first:before:top-0 not-first:before:right-3 not-first:before:left-3 not-first:before:h-px not-first:before:bg-border">
              <span className="grid size-[26px] shrink-0 place-items-center rounded-[9px] bg-destructive/12 text-destructive">
                <TriangleAlert className="size-[15px]" />
              </span>
              <div className="min-w-0">
                <div className="text-[11.5px] text-faint">{i.source}</div>
                <div className="selectable text-[12.5px] select-text">{i.message}</div>
              </div>
            </div>
          ))}
        </Card>
      )}
      <List>
        <NavRow icon={Pencil} label="Enter details manually" onClick={() => nav.push({ name: "edit-server" })} />
        <NavRow icon={WandSparkles} label="Set up my own server" sub="Debian, Ubuntu or Arch Linux VPS" onClick={() => nav.push({ name: "setup" })} />
      </List>
    </Page>
  );
}

function Code({ text }: { text: string }) {
  return (
    <div className="selectable relative rounded-xl bg-fill py-2.5 pr-11 pl-3 font-mono text-[11.5px] leading-relaxed break-all whitespace-pre-wrap select-text">
      {text}
      <div className="absolute top-1 right-1">
        <IconButton label="Copy" onClick={() => api.copyText(text).then(() => toast("Copied"))}>
          <Copy />
        </IconButton>
      </div>
    </div>
  );
}

function Step({ n, title, children }: { n: number; title: string; children: ReactNode }) {
  return (
    <Card className="gap-2.5 p-4">
      <div className="flex items-center gap-2.5 text-[13.5px] font-semibold">
        <span className="grid size-[22px] shrink-0 place-items-center rounded-full bg-primary/14 text-[11.5px] font-semibold text-primary">{n}</span>
        {title}
      </div>
      <div className="flex flex-col gap-2.5 text-[12.5px] leading-relaxed text-muted-foreground">{children}</div>
    </Card>
  );
}

export function ServerSetup({ nav }: { nav: Nav }) {
  const [saved, setSaved] = useState<string | null>(null);
  return (
    <Page title="Set up your server" onBack={nav.back}>
      <Hint>Installs sing-box with VLESS + REALITY. No domain or certificate needed. About a minute.</Hint>
      <Step n={1} title="Get a VPS">
        <p className="m-0">Debian 12/13, Ubuntu 22.04/24.04 or Arch Linux, 512 MB RAM or more, located where you want your exit IP.</p>
      </Step>
      <Step n={2} title="Save the installer">
        <p className="m-0">Verifies its download against a pinned SHA-256 and can be removed with --uninstall.</p>
        <Button
          className="self-start"
          onClick={async () => {
            try {
              const p = await api.saveServerScript();
              if (p) setSaved(p);
            } catch (e) {
              toast(errorText(e), "error");
            }
          }}
        >
          <FileText /> Save install script…
        </Button>
        {saved && <p className="selectable m-0 select-text">Saved to {saved}</p>}
      </Step>
      <Step n={3} title="Run it on the server">
        <p className="m-0">From PowerShell (replace YOUR_SERVER_IP):</p>
        <Code text={"scp lotus-server-install.sh root@YOUR_SERVER_IP:\nssh root@YOUR_SERVER_IP \"bash lotus-server-install.sh\""} />
        <p className="m-0">Optionally add Hysteria2 (UDP):</p>
        <Code text={'ssh root@YOUR_SERVER_IP "bash lotus-server-install.sh --with-hysteria2"'} />
      </Step>
      <Step n={4} title="Import the result">
        <p className="m-0">Copy the lotus:// link it prints, then:</p>
        <Button className="self-start" onClick={() => nav.replace({ name: "add-server" })}>
          <ClipboardPaste /> Paste the link
        </Button>
      </Step>
    </Page>
  );
}
