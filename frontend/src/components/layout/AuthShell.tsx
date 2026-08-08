import { Outlet } from "react-router";
import { Logo } from "@/components/brand/Logo";
import {
  Folder,
  Flag,
  FileText,
  Zap,
  Users,
  ChartColumn
 } from "lucide-react";

import { useTranslation, Trans } from "react-i18next";

const AUTH_FEATURES = [
  { icon: Folder, key: "workspaces" },
  { icon: Flag, key: "kanban" },
  { icon: FileText, key: "writeups" },
  { icon: Zap, key: "realtime" },
  { icon: Users, key: "teams" },
  { icon: ChartColumn, key: "stats" },
];

function BrandSide() {
  const { t } = useTranslation("auth");

  return (
    <aside className="hidden flex-col overflow-y-auto border-r border-border bg-bg-elev px-10 py-10 lg:flex">
      <div className="mx-auto flex w-full max-w-xl flex-col gap-8">
        <Logo size="lg" type="noBg" />

        <div className="flex flex-col gap-3">
          <h1 className="text-display font-semibold leading-tight tracking-hero">
            <Trans
              t={t}
              i18nKey="side.title"
              components={{ ember: <span className="text-ember-text" /> }}
            />
          </h1>
          <p className="text-base text-muted-foreground">{t("side.description")}</p>
        </div>

        <div className="flex flex-col gap-5">
          {AUTH_FEATURES.map(({ icon: Icon, key }) => (
            <div key={key} className="flex flex-row items-start gap-4 py-1">
              <div className="flex size-9 shrink-0 items-center justify-center rounded-md bg-ember-soft text-ember-text">
                <Icon className="size-4" />
              </div>
              <div className="flex flex-col gap-0.5">
                <span className="text-h3 font-semibold">
                  {t(`side.features.${key}.title`)}
                </span>
                <span className="text-base text-muted-foreground">
                  {t(`side.features.${key}.description`)}
                </span>
              </div>
            </div>
          ))}
        </div>
      </div>
    </aside>
  );
}

export function AuthShell() {
  return (
    <div className="grid min-h-dvh lg:grid-cols-2">
      <BrandSide />
      <main className="flex items-center justify-center px-6 py-10">
        <div className="w-full max-w-auth">
          <Outlet />
        </div>
      </main>
    </div>
  );
}
