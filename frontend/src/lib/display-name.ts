/** "First Last" when the account has a name, otherwise its handle. Never "null null". */
export function displayName(person: Readonly<{ first_name?: string | null; last_name?: string | null; username?: string; handle?: string }>): string {
  const full = [person.first_name, person.last_name].filter((part) => part?.trim()).join(" ");
  return full || person.username || person.handle || "";
}
