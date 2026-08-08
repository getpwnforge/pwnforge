import { useTranslation } from "react-i18next";


export function LoginPage() {
  const { t } = useTranslation('auth')
  return (
    <div className="flex flex-col items-center gap-4 h-full w-full">
      <h1 className="text-page font-bold">{t("login.title")}</h1>
      <p className="text-base text-muted-foreground">
        {t("login.subtitle")}
      </p>
    </div>
  );
}
