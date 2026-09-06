"use client";

import type { ReactNode } from "react";
import { Link } from "react-router";
import { Trans, useTranslation } from "react-i18next";
import {
  ArrowRight,
  Building2,
  ChartColumn,
  Check,
  FileText,
  Flag,
  Folder,
  GraduationCap,
  Play,
  User,
  Users,
  Zap,
} from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { ROUTES } from "@/lib/routes";

/** Brand names, deliberately outside i18n: they are spelled the same everywhere. */
// HTB & THM are commented out because they are not yet supported by PwnForge. Their APIs need an Enterprise plan to be used, which is not available for free.
// Root-Me and CTFtime are supported and can be used without an Enterprise plan.
const PLATFORMS = [
  // "HackTheBox",
  // "TryHackMe",
  "Root-Me",
  "CTFtime",
  // "HTB Academy",
];

const TRUST_POINTS = ["selfHostable", "openSource", "exportAnytime"] as const;

const PRODUCT_FEATURES = [
  { key: "workspaces", icon: Folder },
  { key: "kanban", icon: Flag },
  { key: "writeups", icon: FileText },
  { key: "realtime", icon: Zap },
  { key: "teams", icon: Users },
  { key: "stats", icon: ChartColumn },
] as const;

const SOLUTION_CASES = [
  { key: "solo", icon: User },
  { key: "ctfTeam", icon: Flag },
  { key: "studentClub", icon: GraduationCap },
  { key: "redTeam", icon: Building2 },
] as const;

/**
 * Community figures.
 *
 * `null` renders a dash on purpose: these are public claims about adoption and
 * must come from a real count before they go up, not from a nice-looking
 * constant. Wire them to the instance stats endpoint when it exists.
 */
const COMMUNITY_STATS: { key: string; value: string | null }[] = [
  { key: "activeTeams", value: null },
  { key: "countries", value: null },
  { key: "publishedWriteups", value: null },
  { key: "ctfCovered", value: null },
];

function Hero() {
  const { t } = useTranslation("landing");

  return (
    <section className="flex w-full max-w-landing flex-col items-center gap-6 py-10 text-center lg:py-16">
      <h1 className="max-w-3xl text-display-sm font-semibold leading-tight tracking-hero lg:text-display-lg">
        <Trans
          t={t}
          i18nKey="hero.title"
          components={{ ember: <span className="text-ember-text" /> }}
        />
      </h1>

      <p className="max-w-2xl text-base text-muted-foreground">{t("hero.subtitle")}</p>

      <div className="flex flex-col items-stretch gap-3 sm:flex-row sm:items-center">
        <Button asChild size="lg">
          <Link to={ROUTES.register}>
            {t("hero.primaryCta")}
            <ArrowRight />
          </Link>
        </Button>
        <Button asChild variant="secondary" size="lg">
          <Link to={{ pathname: ROUTES.landing, hash: "#product" }}>
            <Play />
            {t("hero.secondaryCta")}
          </Link>
        </Button>
      </div>

      <ul className="flex flex-wrap items-center justify-center gap-x-6 gap-y-2 text-sm text-muted-foreground">
        {TRUST_POINTS.map((point) => (
          <li key={point} className="flex items-center gap-1.5">
            <Check className="size-3.5 text-ember-text" />
            {t(`hero.trust.${point}`)}
          </li>
        ))}
      </ul>
    </section>
  );
}

function Platforms() {
  const { t } = useTranslation("landing");

  return (
    <section className="flex w-full max-w-landing flex-col items-center gap-5">
      <span className="font-mono text-xs uppercase tracking-wider text-text-subtle">
        {t("platforms.title")}
      </span>
      <ul className="flex flex-wrap items-center justify-center gap-x-10 gap-y-3 font-mono text-h2 font-semibold text-text-subtle">
        {PLATFORMS.map((platform) => (
          <li key={platform}>{platform}</li>
        ))}
      </ul>
    </section>
  );
}

function SectionHeading({
  eyebrow,
  title,
  subtitle,
}: Readonly<{
  eyebrow: string;
  title: ReactNode;
  subtitle?: string;
}>) {
  return (
    <div className="flex flex-col items-center gap-3 text-center">
      <Badge variant="muted">{eyebrow}</Badge>
      <h2 className="max-w-2xl text-display font-semibold tracking-hero">{title}</h2>
      {subtitle ? <p className="max-w-2xl text-base text-muted-foreground">{subtitle}</p> : null}
    </div>
  );
}

function Product() {
  const { t } = useTranslation("landing");

  return (
    <section id="product" className="flex w-full max-w-landing scroll-mt-20 flex-col gap-10">
      <SectionHeading eyebrow={t("product.eyebrow")} title={t("product.title")} />

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {PRODUCT_FEATURES.map(({ key, icon: Icon }) => (
          <Card key={key}>
            <CardHeader className="gap-3">
              <div className="flex size-9 items-center justify-center rounded-md bg-ember-soft text-ember-text">
                <Icon className="size-4" />
              </div>
              <CardTitle>{t(`product.features.${key}.title`)}</CardTitle>
              <CardDescription>{t(`product.features.${key}.description`)}</CardDescription>
            </CardHeader>
          </Card>
        ))}
      </div>
    </section>
  );
}

function Solutions() {
  const { t } = useTranslation("landing");

  return (
    <section id="solutions" className="flex w-full max-w-landing scroll-mt-20 flex-col gap-10">
      <SectionHeading
        eyebrow={t("solutions.eyebrow")}
        title={
          <Trans
            t={t}
            i18nKey="solutions.title"
            components={{ ember: <span className="text-ember-text" /> }}
          />
        }
        subtitle={t("solutions.subtitle")}
      />

      <div className="grid gap-4 sm:grid-cols-2">
        {SOLUTION_CASES.map(({ key, icon: Icon }) => (
          <Card key={key}>
            <CardHeader className="gap-3">
              <div className="flex items-center gap-3">
                <div className="flex size-9 items-center justify-center rounded-md bg-surface-2 text-foreground">
                  <Icon className="size-4" />
                </div>
                <Badge variant="muted">{t(`solutions.cases.${key}.tag`)}</Badge>
              </div>
              <CardTitle className="text-h2">{t(`solutions.cases.${key}.title`)}</CardTitle>
              <CardDescription>{t(`solutions.cases.${key}.description`)}</CardDescription>
            </CardHeader>
          </Card>
        ))}
      </div>
    </section>
  );
}

function Community() {
  const { t } = useTranslation("landing");

  return (
    <section id="community" className="flex w-full max-w-landing scroll-mt-20 flex-col gap-10">
      <SectionHeading eyebrow={t("community.eyebrow")} title={t("community.title")} />

      <dl className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {COMMUNITY_STATS.map(({ key, value }) => (
          <div
            key={key}
            className="flex flex-col items-center gap-1 rounded-md bg-bg-elev px-4 py-6 ring-1 ring-foreground/10"
          >
            <dt className="order-2 text-sm text-muted-foreground">
              {t(`community.stats.${key}`)}
            </dt>
            <dd className="order-1 text-stat font-semibold tabular-nums">{value ?? "—"}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

function FinalCta() {
  const { t } = useTranslation("landing");

  return (
    <section className="flex w-full max-w-landing flex-col items-center gap-4 rounded-lg bg-bg-elev px-6 py-12 text-center ring-1 ring-foreground/10">
      <h2 className="text-display-sm font-semibold tracking-hero">{t("finalCta.title")}</h2>
      <p className="text-base text-muted-foreground">{t("finalCta.subtitle")}</p>
      <Button asChild size="lg">
        <Link to={ROUTES.register}>
          {t("finalCta.button")}
          <ArrowRight />
        </Link>
      </Button>
    </section>
  );
}

export function LandingPage() {
  return (
    <>
      <Hero />
      <Platforms />
      <Product />
      <Solutions />
      <Community />
      <FinalCta />
    </>
  );
}
