import { useState, type SubmitEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useTurnstile } from "@/hooks/useTurnstile";
import { TurnstileWidget } from "@/components/common/TurnstileWidget";

type Category = "general" | "self_hosting" | "billing" | "press" | "other";


export function ContactPage() {
  const { t, i18n } = useTranslation("public");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [category, setCategory] = useState<Category>("general");
  const [message, setMessage] = useState("");
  const [status, setStatus] = useState<"idle" | "submitting" | "success" | "error">("idle");
  const turnstile = useTurnstile();
  const canSubmit = status !== "submitting" && turnstile.isReady;

  async function handleSubmit(e: SubmitEvent<HTMLFormElement>) {
    e.preventDefault();
    if (!canSubmit) return;

    setStatus("submitting");
    try {
      const res = await fetch("/api/v1/contact", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name,
          email,
          category,
          message,
          turnstile_token: turnstile.token ?? "",
          locale: i18n.language,
        }),
      });
      if (!res.ok) throw new Error(`Contact form submission failed (${res.status})`);
      setStatus("success");
    } catch {
      turnstile.reset();
      setStatus("error");
    }
  }

  if (status === "success") {
    return (
      <div className="max-w-lg mx-auto py-24 text-center">
        <h1 className="text-h1 font-semibold text-text">{t("contact.successTitle")}</h1>
        <p className="text-muted-foreground mt-2">{t("contact.successBody")}</p>
      </div>
    );
  }

  return (
    <div className="max-w-lg mx-auto px-6 py-16">
      <h1 className="text-page font-semibold text-text mb-2">{t("contact.title")}</h1>
      <p className="text-muted-foreground text-sm mb-8">{t("contact.subtitle")}</p>

      <form onSubmit={handleSubmit} className="flex flex-col gap-4">
        <div>
          <label className="block text-sm font-medium text-text mb-1.5" htmlFor="name">{t("contact.name")}</label>
          <input id="name" required value={name} onChange={(e) => setName(e.target.value)}
                 className="w-full rounded-md border border-border bg-surface px-3 py-2 text-sm text-text" />
        </div>

        <div>
          <label className="block text-sm font-medium text-text mb-1.5" htmlFor="email">{t("contact.email")}</label>
          <input id="email" type="email" required value={email} onChange={(e) => setEmail(e.target.value)}
                 className="w-full rounded-md border border-border bg-surface px-3 py-2 text-sm text-text" />
        </div>

        <div>
          <label className="block text-sm font-medium text-text mb-1.5" htmlFor="category">{t("contact.category")}</label>
          <select id="category" value={category} onChange={(e) => setCategory(e.target.value as Category)}
                  className="w-full rounded-md border border-border bg-surface px-3 py-2 text-sm text-text">
            <option value="general">{t("contact.categories.general")}</option>
            <option value="self_hosting">{t("contact.categories.selfHosting")}</option>
            <option value="billing">{t("contact.categories.billing")}</option>
            <option value="press">{t("contact.categories.press")}</option>
            <option value="other">{t("contact.categories.other")}</option>
          </select>
        </div>

        <div>
          <label className="block text-sm font-medium text-text mb-1.5" htmlFor="message">{t("contact.message")}</label>
          <textarea id="message" required rows={5} value={message} onChange={(e) => setMessage(e.target.value)}
                    className="w-full rounded-md border border-border bg-surface px-3 py-2 text-sm text-text resize-none" />
        </div>

        <TurnstileWidget siteKey={turnstile.siteKey} widgetRef={turnstile.ref} onSuccess={turnstile.setToken} />

        {status === "error" && <p className="text-sm text-danger-text">{t("contact.error")}</p>}

        <Button type="submit" disabled={!canSubmit}>
          {status === "submitting" ? t("contact.sending") : t("contact.submit")}
        </Button>
      </form>

      <div className="mt-8 pt-6 border-t border-border text-sm text-muted-foreground">
        <p>{t("contact.securityNotice")} <a href="mailto:security@pwnforge.app" className="text-ember hover:text-ember-hover hover:underline">security@pwnforge.app</a></p>
      </div>
    </div>
  );
}
