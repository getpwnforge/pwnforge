---
slug: legal
lang: fr
title: Mentions légales
description: Éditeur, hébergement, licence et signalement de contenu pour votre instance PwnForge.
version: "0.1-template"
updated: "[DATE]"
---

> **⚠️ Gabarit, pas un conseil juridique.** Base de départ pour les
> opérateurs qui self-hébergent PwnForge, construite à partir du document de
> l'instance cloud officielle sous droit français/européen. Chaque champ
> entre `[CROCHETS]` doit être rempli avec vos propres informations, et
> chaque référence juridique doit être vérifiée au regard du droit qui
> s'applique réellement à vous : votre pays d'établissement, pas la France,
> sauf si vous y êtes vous-même établi. Faites relire ce document par un
> professionnel avant de le publier, surtout si votre instance compte de
> vrais utilisateurs au-delà d'un petit groupe privé.

# Mentions légales

## À propos de ce document

Trois documents encadrent généralement l'utilisation d'une instance
self-hébergée :

| Document | Contenu |
| --- | --- |
| **Mentions légales** (ce document) | Qui exploite l'instance, où elle est hébergée, sous quelle licence, comment contacter l'exploitant |
| Politique de confidentialité | Quelles données sont traitées, pourquoi, combien de temps, et quels droits les utilisateurs ont |
| Conditions d'utilisation | Règles d'usage, contenu, modération, responsabilité |

---

## 1. Statut du service

Précisez ici si votre instance est personnelle/non commerciale, exploitée
par une organisation, ou constitue une offre commerciale. Ce point change
les obligations qui s'appliquent à vous (droit de la consommation,
immatriculation obligatoire, TVA...), qu'aucun des gabarits fournis ici ne
couvre pour un cas commercial.

---

## 2. Éditeur du service

| Élément | Valeur |
| --- | --- |
| Exploitant | `[VOTRE NOM OU RAISON SOCIALE]` |
| Qualité | `[ex. personne physique, éditeur non professionnel / société immatriculée]` |
| Adresse de contact | `[VOTRE EMAIL DE CONTACT]` |
| Coordonnées complètes | `[Où elles sont conservées — ex. transmises à votre hébergeur, ou publiées ici directement si vous êtes une entreprise immatriculée]` |

Si vous opérez en tant que particulier plutôt qu'en tant qu'entreprise
immatriculée, vérifiez si l'équivalent local de la LCEN française (ou
l'absence d'équivalent) vous permet de ne pas rendre publiques votre
identité civile et votre adresse postale, à condition que votre hébergeur
les détienne. Ne présumez pas que la règle française (article 1-1, II de la
loi n° 2004-575) s'applique à vous si vous n'êtes pas effectivement établi
en France.

---

## 3. Hébergement et infrastructure

### 3.1 Hébergeur de l'application

| Élément | Valeur |
| --- | --- |
| Société | `[VOTRE HÉBERGEUR]` |
| Siège social | `[ADRESSE]` |
| Localisation des serveurs | `[PAYS / RÉGION]` |

### 3.2 Autres composants d'infrastructure

Listez chaque tiers dont l'infrastructure fait réellement transiter ou
héberge le trafic ou les données de votre instance : DNS, CDN, envoi
d'emails, analytics, stockage d'objets. Une ligne par prestataire, un rôle
réel par ligne :

| Prestataire | Rôle | Localisation |
| --- | --- | --- |
| `[ex. votre prestataire DNS/CDN]` | `[ex. DNS, CDN, pare-feu applicatif]` | `[Localisation]` |
| `[ex. votre prestataire d'emailing]` | `[ex. envoi d'emails transactionnels]` | `[Localisation]` |

---

## 4. Contact

| Sujet | Adresse |
| --- | --- |
| Support, questions d'usage | `[VOTRE EMAIL SUPPORT]` |
| Signalement de contenu illicite ou contraire aux CGU | `[VOTRE EMAIL ABUSE]` |
| Signalement de vulnérabilité de sécurité | `[VOTRE EMAIL SÉCURITÉ]` |
| Demandes relatives aux données personnelles | `[VOTRE EMAIL VIE PRIVÉE]` |

---

## 5. Objet du service

PwnForge est une plateforme collaborative pour les équipes de CTF et les
praticiens de la sécurité offensive : suivi de challenges, writeups
collaboratifs, statistiques de workspace. Le service ne fournit aucune
infrastructure d'attaque ni environnement d'exécution de code fourni par les
utilisateurs — indiquez-le clairement, c'est la base qui permet de vous
considérer comme hébergeur plutôt qu'éditeur de ce que les utilisateurs font
des informations qu'ils organisent sur la plateforme.

---

## 6. Licence du code source

Le cœur de PwnForge est distribué sous licence **AGPL v3**. Si vous n'avez
pas modifié le code source, dites-le et renvoyez vers le dépôt d'origine. Si
vous exploitez une version modifiée, **l'article 13 de l'AGPL v3 vous oblige
à mettre votre code source modifié à disposition** de chaque utilisateur de
votre instance — ce n'est pas optionnel, et c'est la seule obligation de
tout ce document qui a de vraies conséquences juridiques en cas de non-respect.

---

## 7. Marque et identité visuelle

À moins de détenir vous-même des droits de marque sur le nom et le logo
PwnForge, vous ne pouvez accorder aucune permission à leur sujet : cette
politique appartient au projet d'origine. Indiquez plutôt que vous exploitez
une instance de PwnForge, projet open source, et renvoyez vers la politique
de marque du projet d'origine pour tout ce qui concerne l'usage de son nom
et de son logo. Si vous avez renommé ou rebrandé votre fork, décrivez ici
votre propre politique de nom/logo.

---

## 8. Contenu éditorial

Le contenu que vous produisez (documentation, annonces) est protégé par le
droit d'auteur classique. Le contenu publié par les utilisateurs leur
appartient — renvoyez vers vos conditions d'utilisation.

---

## 9. Signalement de contenu illicite

Indiquez que vous agissez en tant qu'hébergeur du contenu publié par les
utilisateurs, sans obligation générale de le surveiller, et que vous agissez
sur signalement d'un contenu manifestement illicite envoyé à `[VOTRE EMAIL
ABUSE]`. Si votre juridiction prévoit une sanction spécifique pour les
signalements abusifs de mauvaise foi (la LCEN française le fait, à l'article
6, I, 4), vérifiez qu'un équivalent existe là où vous opérez avant d'en
citer un.

---

## 10. Réclamations

Indiquez où adresser les réclamations (`[VOTRE EMAIL SUPPORT]`) et si un
dispositif de médiation de la consommation s'applique à vous. Ce n'est
généralement pas le cas pour un service gratuit, non commercial et
personnel, mais cela dépend entièrement du droit de la consommation
applicable localement, pas du droit français par défaut.

---

## 11. Liens hypertextes

Clause standard : aucun contrôle ni responsabilité sur les ressources
tierces, le lien vers votre service est autorisé en l'absence de suggestion
trompeuse de partenariat.

---

## 12. Données personnelles

Renvoyez vers votre politique de confidentialité. Nommez ici votre autorité
de contrôle compétente (votre autorité nationale de protection des données
si vous êtes dans l'UE/EEE, ou l'équivalent pertinent pour votre pays) — ne
présumez pas de la CNIL par défaut sauf si vous êtes effectivement établi en
France.

---

## 13. Accessibilité

Indiquez votre niveau cible d'accessibilité (ex. WCAG 2.1 AA) et si vous
avez réellement été audité à ce sujet. Ne revendiquez pas un niveau de
conformité que vous n'avez pas vérifié.

---

## 14. Instances dérivées de la vôtre

Si votre code source est public (ce qui est obligatoire si vous avez
modifié du code sous AGPL et l'avez rendu accessible en réseau, cf. section
6), indiquez clairement que vous n'êtes pas responsable des instances
supplémentaires déployées par des tiers à partir de votre code, et que
chaque exploitant de ce type est responsable du traitement des données de
sa propre instance.

---

## 15. Crédits

Listez les principaux composants open source sur lesquels vous vous appuyez
et leurs licences.

---

## 16. Portée internationale et droit applicable

Indiquez à partir de quel droit ce document est rédigé (votre lieu
d'établissement, pas nécessairement la France), et qu'une règle impérative
plus favorable du pays de résidence d'un utilisateur reste applicable
lorsqu'elle existe.

---

## 17. Historique des versions

| Version | Date | Modifications |
| --- | --- | --- |
| 0.1 | `[DATE]` | Version initiale |
