// Casivell's strings, in German and English.
//
// # What is translated, and what is deliberately not
//
// **Statutory terms stay German in both languages.** Lohnsteuer, Solidaritätszuschlag,
// Vorsorgepauschale, Beitragsbemessungsgrenze — these are the words printed on the payslip, the
// Steuerbescheid and the Bescheinigung the reader is holding. Rendering "Lohnsteuer" as "wage
// tax" would make a figure *harder* to reconcile against the document it came from, which is
// the one job this tool has. An English-speaking user in Germany needs the German noun and an
// English explanation of it, not an English noun with no counterpart on any form.
//
// So `TERMS` is shared by both languages, and only the surrounding prose is translated. Where
// a term needs glossing, the English text glosses it in passing rather than replacing it.
//
// **Numbers stay in German format** — 1.234,56 € — in both languages, for the same reason.
// Every document the figures will be compared against uses it. An English reader gets English
// sentences around German numerals, which is what a German payslip looks like to them anyway.

export const TERMS = {
  gross: "Bruttolohn",
  incomeTax: "Lohnsteuer",
  solidarity: "Solidaritätszuschlag",
  churchTax: "Kirchensteuer",
  contributions: "Sozialabgaben",
  pension: "Rentenversicherung",
  health: "Krankenversicherung",
  care: "Pflegeversicherung",
  unemployment: "Arbeitslosenversicherung",
  net: "Nettolohn",
  annualGross: "Jahresarbeitslohn",
  allowances: "Tabellenfreibeträge",
  vorsorge: "Vorsorgepauschale",
  taxable: "zu versteuernder Betrag",
  annualTax: "Jahreslohnsteuer",
  supplementary: "Zusatzbeitrag",
  datenstand: "Datenstand",
};

export const MESSAGES = {
  de: {
    lang: "de",
    tagline: "Brutto zu Netto, gerechnet nach dem BMF-Programmablaufplan. " +
             "Nichts verlässt dieses Gerät.",
    formPayslip: "Brutto-Netto",
    formClasses: "Steuerklassen",
    formProject: "Projektion",
    fieldGross: "Bruttolohn (€ / Monat)",
    fieldPartner: "Bruttolohn Partner (€ / Monat)",
    fieldClass: "Steuerklasse",
    fieldLand: "Bundesland",
    fieldYear: "Jahr",
    fieldAge: "Alter",
    fieldChildren: "Kinder",
    fieldKvz: "Zusatzbeitrag (%)",
    fieldChurch: "Kirchensteuer",
    fieldExpenses: "Ausgaben (€ / Monat)",
    fieldYears: "Jahre",
    fieldReturn: "Rendite (%)",
    fieldPayGrowth: "Lohnwachstum (%)",
    yes: "ja",
    no: "nein",
    explainHint: "Auf eine Zeile klicken, um zu sehen, welche Regel sie erzeugt hat.",
    why: {
      incomeTax: { law: "§ 39b EStG · BMF-Programmablaufplan",
        text: "Der PAP rechnet den Monatslohn aufs Jahr hoch, zieht die Freibeträge und die " +
              "Vorsorgepauschale ab, wendet den Tarif des § 32a auf den Rest an und teilt " +
              "wieder durch zwölf. Die Zwischenschritte stehen unten." },
      solidarity: { law: "SolzG 1995 · § 3, § 4",
        text: "5,5 % der Lohnsteuer, aber erst oberhalb der Freigrenze und dann zunächst " +
              "abgemildert. Unterhalb bleibt er null — deshalb steht hier bei den meisten " +
              "Gehältern 0,00 €." },
      churchTax: { law: "Kirchensteuergesetze der Länder · § 51a EStG",
        text: "8 % in Bayern und Baden-Württemberg, 9 % sonst — bemessen auf die Lohnsteuer, " +
              "die mit Kinderfreibeträgen gerechnet wird (§ 51a Abs. 2). Die Kappung, die die " +
              "meisten Landeskirchen anwenden, ist nicht abgebildet." },
      pension: { law: "§ 158 SGB VI",
        text: "18,6 % geteilt durch zwei, bis zur Beitragsbemessungsgrenze." },
      health: { law: "§ 241 SGB V",
        text: "14,6 % geteilt durch zwei, plus die Hälfte des kassenindividuellen " +
              "Zusatzbeitrags." },
      care: { law: "§ 55 SGB XI",
        text: "3,6 % geteilt durch zwei, mit Zuschlag für Kinderlose ab 23 und Abschlag je " +
              "Kind ab dem zweiten. In Sachsen trägt der Arbeitnehmer einen Punkt mehr." },
      unemployment: { law: "§ 341 SGB III",
        text: "2,6 % geteilt durch zwei, bis zur selben Grenze wie die Rentenversicherung." },
      contributions: { law: "SGB III / V / VI / XI",
        text: "Die Summe der vier Zweige. Jeder ist oben einzeln aufgeführt." },
      net: { law: "—",
        text: "Brutto abzüglich aller Posten darüber. Vorläufig: der Jahresausgleich rechnet ab." },
    },
    captionPayslip: "Lohnabrechnung: Brutto, Abzüge und Nettolohn",
    captionClasses: "Steuerklassenvergleich: monatliche Abzüge, Netto und Veranlagung",
    captionProject: "Projektion: Vermögen und Rentenanwartschaft je Jahr",
    explainShow: "— Erklärung anzeigen",
    perMonth: "÷ 12 = Lohnsteuer im Monat",
    updateAvailable: "Eine neuere Fassung ist verfügbar — möglicherweise mit geänderten " +
                     "Rechtsständen.",
    reload: "Neu laden",
    noAdvice: "Keine Steuerberatung",
    noAdviceRest: "(§§ 1–4 StBerG). Vorläufige Werte; der Jahresausgleich rechnet ab.",
    limitations: "Was dieser Rechner nicht kann, steht in",
    offline: "Funktioniert offline. Der Rechtsstand oben gilt dann für die zuletzt geladene " +
             "Fassung — bei bestehender Verbindung wird auf eine neuere hingewiesen.",
    inForce: "gesetzlich in Kraft",
    legalBasis: "Rechtsstand",
    announceNet: (net, gross) => `${TERMS.net} ${net} von ${gross} brutto.`,
    // Steuerklassen
    classesLede: total => `Die Jahres-Einkommensteuer beträgt <strong>${total}</strong> — bei ` +
      `allen drei Varianten. Die Steuerklasse entscheidet, <em>wann</em> gezahlt wird und ` +
      `von wem, nicht wie viel.`,
    colVariant: "Variante", colHigher: "höher", colLower: "niedriger",
    colNetTotal: "Netto ges.", colAssessment: "Veranlagung",
    owes: amount => `${amount} nachzahlen`,
    back: amount => `${amount} zurück`,
    noFactor: "Der Faktor ist nur unter 1 zulässig (§ 39f Abs. 1). Bei etwa gleichen " +
              "Gehältern zieht Klasse IV bereits richtig ab, es gibt nichts zu korrigieren.",
    whySameTitle: "Warum die Summe gleich bleibt",
    whySame: "§ 32a Abs. 5 EStG bestimmt die Jahressteuer eines Ehepaares aus dem " +
             "gemeinsamen Einkommen. Keine Kombination von Klassen ändert daran etwas — der " +
             "Jahresausgleich holt die Differenz in beide Richtungen zurück.",
    whyMattersTitle: "Wo die Wahl trotzdem zählt",
    whyMatters: "Lohnersatzleistungen werden aus dem <em>Netto</em> berechnet: Elterngeld, " +
                "Arbeitslosen- und Krankengeld fallen in Klasse V spürbar niedriger aus als " +
                "in Klasse III — bei gleicher Jahressteuer.",
    announceLiability: total =>
      `Jahres-Einkommensteuer ${total}, gleich bei allen drei Varianten.`,
    // Projektion
    colYear: "Jahr", colNetMonthly: "Netto/mtl.", colSaved: "Sparrate",
    colWealth: "Vermögen", colPension: "Rente/mtl.",
    whatsHereTitle: "Was hier steht",
    whatsHere: last => `Jede Zeile ist ein Jahresende. Mit <strong>*</strong> markierte Jahre ` +
      `beruhen auf fortgeschriebenen Rechengrößen, nicht auf geltendem Recht — jenseits von ` +
      `${last} gibt es keine beschlossenen Werte.`,
    turnsTitle: "Das Vermögen kehrt um",
    turns: (peak, year) => `Der höchste Stand liegt bei ${peak} im Jahr ${year}, danach fällt ` +
      `er. Wenn die Ausgaben mit den Preisen steigen und der Lohn nicht, überholen sie ` +
      `irgendwann das Netto.`,
    missingTitle: "Was nicht enthalten ist",
    missing: "Immobilien, Elterngeld, Kapitalerträge und Lebensereignisse rechnet nur die " +
             "Kommandozeile. Renditen sind Annahmen, keine Zusagen.",
    chartCaption: "Geldvermögen, nominal. Gestrichelt: Jahre auf fortgeschriebenen " +
                  "Rechengrößen.",
    chartLabel: (from, to, max) => `Vermögensverlauf von ${from} bis ${to}, Höchststand ${max}`,
    announceProjection: (years, wealth, pension) =>
      `Nach ${years} Jahren ${wealth} Vermögen und ${pension} monatliche Rentenanwartschaft.`,
    errors: {
      "-1": "Für dieses Jahr liegen keine Daten vor.",
      "-2": "Unbekannte Steuerklasse.",
      "-3": "Unbekanntes Bundesland.",
      "-4": "Eine Eingabe liegt außerhalb des zulässigen Bereichs. Kinder setzen die " +
            "Elterneigenschaft voraus.",
      "-5": "Die Rechnung hat den darstellbaren Bereich verlassen.",
      unknown: "Unbekannter Fehler.",
    },
  },

  en: {
    lang: "en",
    tagline: "Gross to net, computed from the BMF Programmablaufplan. Nothing leaves this " +
             "device.",
    formPayslip: "Gross to net",
    formClasses: "Tax classes",
    formProject: "Projection",
    fieldGross: "Gross pay (€ / month)",
    fieldPartner: "Partner's gross pay (€ / month)",
    fieldClass: "Tax class (Steuerklasse)",
    fieldLand: "State (Bundesland)",
    fieldYear: "Year",
    fieldAge: "Age",
    fieldChildren: "Children",
    fieldKvz: "Zusatzbeitrag (%)",
    fieldChurch: "Church tax (Kirchensteuer)",
    fieldExpenses: "Spending (€ / month)",
    fieldYears: "Years",
    fieldReturn: "Return (%)",
    fieldPayGrowth: "Pay growth (%)",
    yes: "yes",
    no: "no",
    explainHint: "Select a row to see the provision that produced it.",
    why: {
      incomeTax: { law: "§ 39b EStG · BMF Programmablaufplan",
        text: "The PAP annualises the monthly wage, deducts the table allowances and the " +
              "Vorsorgepauschale, applies the § 32a tariff to what is left, and divides by " +
              "twelve again. The intermediate steps are below." },
      solidarity: { law: "SolzG 1995 · § 3, § 4",
        text: "5.5 % of the Lohnsteuer — but only above the Freigrenze, and tapered at first. " +
              "Below it there is none, which is why most salaries show 0,00 € here." },
      churchTax: { law: "Kirchensteuergesetze der Länder · § 51a EStG",
        text: "8 % in Bayern and Baden-Württemberg, 9 % elsewhere — levied on the Lohnsteuer " +
              "recomputed *with* Kinderfreibeträge (§ 51a Abs. 2). The Kappung most " +
              "Landeskirchen apply is not modelled." },
      pension: { law: "§ 158 SGB VI",
        text: "18.6 % halved between employer and employee, up to the " +
              "Beitragsbemessungsgrenze." },
      health: { law: "§ 241 SGB V",
        text: "14.6 % halved, plus half of your fund's own Zusatzbeitrag." },
      care: { law: "§ 55 SGB XI",
        text: "3.6 % halved, with a surcharge for the childless from age 23 and a reduction " +
              "per child from the second. In Sachsen the employee carries one point more." },
      unemployment: { law: "§ 341 SGB III",
        text: "2.6 % halved, up to the same ceiling as the pension contribution." },
      contributions: { law: "SGB III / V / VI / XI",
        text: "The sum of the four branches, each listed separately above." },
      net: { law: "—",
        text: "Gross less everything above it. Provisional: the annual assessment settles the " +
              "difference." },
    },
    captionPayslip: "Payslip: gross, deductions and net pay",
    captionClasses: "Tax class comparison: monthly withholding, net and assessment",
    captionProject: "Projection: wealth and pension entitlement by year",
    explainShow: "— show explanation",
    perMonth: "÷ 12 = Lohnsteuer per month",
    updateAvailable: "A newer version is available — possibly with different statutory data.",
    reload: "Reload",
    noAdvice: "Not tax advice",
    noAdviceRest: "(§§ 1–4 StBerG). Provisional: the annual assessment settles the difference.",
    limitations: "What this calculator cannot do is set out in",
    offline: "Works offline. The statutory basis shown above is then the one the cached " +
             "version was built with; when online, you will be told if a newer one exists.",
    inForce: "enacted law",
    legalBasis: "Statutory basis",
    announceNet: (net, gross) => `${TERMS.net} — net pay ${net} of ${gross} gross.`,
    classesLede: total => `The annual income tax is <strong>${total}</strong> under all three ` +
      `arrangements. The Steuerklasse decides <em>when</em> it is paid and by which spouse, ` +
      `not how much.`,
    colVariant: "Arrangement", colHigher: "higher", colLower: "lower",
    colNetTotal: "Net total", colAssessment: "At assessment",
    owes: amount => `${amount} owed`,
    back: amount => `${amount} back`,
    noFactor: "The Faktor applies only below 1 (§ 39f Abs. 1). With roughly equal salaries, " +
              "class IV already withholds correctly and there is nothing to correct.",
    whySameTitle: "Why the total does not move",
    whySame: "§ 32a Abs. 5 EStG fixes a married couple's annual tax from their combined " +
             "income. No combination of classes changes it — the annual assessment settles " +
             "the difference in either direction.",
    whyMattersTitle: "Where the choice does matter",
    whyMatters: "Wage-replacement benefits are computed from <em>net</em> pay: Elterngeld, " +
                "Arbeitslosengeld and Krankengeld are markedly lower in class V than in " +
                "class III — at the same annual tax.",
    announceLiability: total =>
      `Annual income tax ${total}, the same under all three arrangements.`,
    colYear: "Year", colNetMonthly: "Net/month", colSaved: "Saved/month",
    colWealth: "Wealth", colPension: "Pension/month",
    whatsHereTitle: "What this shows",
    whatsHere: last => `Each row is a year end. Years marked <strong>*</strong> rest on ` +
      `projected statutory figures rather than enacted law — beyond ${last} nothing has been ` +
      `enacted.`,
    turnsTitle: "The wealth line turns",
    turns: (peak, year) => `It peaks at ${peak} in ${year} and falls thereafter. When spending ` +
      `rises with prices and pay does not, spending eventually overtakes net pay.`,
    missingTitle: "What is not included",
    missing: "Property, Elterngeld, capital income and life events are computed only by the " +
             "command line. Returns are assumptions, not promises.",
    chartCaption: "Financial wealth, nominal. Dashed: years resting on projected figures.",
    chartLabel: (from, to, max) => `Wealth from ${from} to ${to}, peaking at ${max}`,
    announceProjection: (years, wealth, pension) =>
      `After ${years} years, ${wealth} in wealth and ${pension} monthly pension entitlement.`,
    errors: {
      "-1": "No statutory data for that year.",
      "-2": "Unknown Steuerklasse.",
      "-3": "Unknown Bundesland.",
      "-4": "An input is outside its permitted range. Children imply Elterneigenschaft.",
      "-5": "The calculation left the representable range.",
      unknown: "Unknown error.",
    },
  },
};

/// The language to start in: an explicit `?lang=`, else a stored choice, else the browser's,
/// else German.
///
/// The URL parameter comes first so a link can carry its language — useful for documentation
/// and for sending someone a figure in a language they read. German is the fallback rather
/// than English because the figures, the terms and the documents they are compared against
/// are all German.
export function initialLanguage(stored, navigatorLanguages = [], requested = null) {
  if (requested && MESSAGES[requested]) return requested;
  if (stored && MESSAGES[stored]) return stored;
  for (const tag of navigatorLanguages) {
    const base = String(tag).slice(0, 2).toLowerCase();
    if (MESSAGES[base]) return base;
  }
  return "de";
}
