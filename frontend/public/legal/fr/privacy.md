---
slug: privacy
lang: fr
title: Politique de confidentialité
description: Données traitées par votre instance PwnForge, finalités, durées de conservation, destinataires et droits des utilisateurs.
version: "0.1-template"
updated: "[DATE]"
---

> **⚠️ Gabarit, pas un conseil juridique.** Base de départ uniquement. Si
> votre instance compte des utilisateurs dans l'UE/EEE ou au Royaume-Uni,
> le RGPD/UK-GDPR s'applique probablement à vous indépendamment de votre
> lieu d'établissement — ne présumez pas le contraire simplement parce que
> votre serveur n'est pas en Europe. Faites relire ce document au regard du
> droit qui s'applique réellement à votre instance et à ses utilisateurs
> avant de le publier.

# Politique de confidentialité

Cette politique complète les mentions légales et les conditions
d'utilisation.

---

## 1. Responsable du traitement

| Élément | Valeur |
| --- | --- |
| Responsable | `[VOUS / VOTRE ORGANISATION]` |
| Pays d'établissement | `[VOTRE PAYS]` |
| Contact protection des données | `[VOTRE EMAIL VIE PRIVÉE]` |

Indiquez si vous avez désigné un délégué à la protection des données (DPO).
La plupart des petites instances exploitées par une seule personne n'en ont
pas besoin au titre de l'article 37 du RGPD, mais vérifiez au regard de
votre échelle réelle et de la nature de ce que vous traitez, pas par simple
présomption.

---

## 2. Périmètre

Indiquez clairement que cette politique couvre uniquement votre instance, et
explicitement **pas** une éventuelle autre instance qu'un tiers exploiterait
à partir de votre code source (sous licence AGPL).

---

## 3. Principes directeurs

Vaut la peine d'être énoncé même brièvement : minimisation, absence de
revente de données, absence de publicité comportementale, lieu réel
d'hébergement des données et — point important compte tenu de
l'architecture de chiffrement des flags de cette plateforme — transparence
sur les **limites** de vos mesures techniques, pas seulement sur leur
existence.

---

## 4. Données que vous traitez

Reprenez cette structure de tableau et ajustez-la à ce que votre instance
collecte réellement — ne recopiez pas telle quelle la liste de l'instance
cloud si vous avez désactivé certaines fonctionnalités (ex. pas
d'intégrations CTF tierces, pas d'emails marketing) :

| Catégorie | Exemples |
| --- | --- |
| Données de compte | Nom d'utilisateur, email, hash du mot de passe, champs de profil optionnels |
| Données d'authentification | Jetons d'accès/rafraîchissement, métadonnées de session (IP, user agent) |
| Données d'usage et journaux | Journaux serveur, journaux d'audit des actions sensibles |
| Contenu généré par les utilisateurs | Writeups, commentaires, données de challenges, pièces jointes |
| Données de support | Contenu des messages envoyés à vos adresses de contact |
| Données d'intégration tierce | Si vous activez des connecteurs de plateformes CTF : identifiants publics, jetons |

Indiquez clairement ce que vous **ne** collectez **pas** : données de
paiement (si l'instance est gratuite), données sensibles au sens de
l'article 9 du RGPD, géolocalisation précise, sauf si l'un de ces points
s'applique réellement à votre déploiement.

---

## 5. Finalités et bases légales

Si le RGPD s'applique à vous, ce tableau est celui qui compte le plus :
chaque finalité a besoin d'une vraie base légale, pas seulement d'un objectif :

| Finalité | Base légale (art. 6 RGPD) |
| --- | --- |
| Création et gestion du compte | Exécution du contrat, art. 6(1)(b) |
| Sécurité, prévention de la fraude et des abus | Intérêt légitime, art. 6(1)(f) |
| Journalisation d'audit | Intérêt légitime, art. 6(1)(f) |
| `[Toute communication marketing que vous envoyez]` | Consentement, art. 6(1)(a) |

---

## 6. Durées de conservation

Autre tableau à garder comme squelette, à remplir avec vos propres chiffres
plutôt que recopié d'ailleurs :

| Catégorie | Durée de conservation |
| --- | --- |
| Compte actif | Durée d'utilisation |
| Compte supprimé | `[Votre délai de grâce, le cas échéant]` |
| Jetons d'accès | `[Durée de vie]` |
| Jetons de rafraîchissement | `[Durée de vie / politique de rotation]` |
| Journaux d'audit | `[Durée de conservation]` |
| Sauvegardes | `[Durée de conservation, politique de rotation]` |

---

## 7. Destinataires et sous-traitants

**Le tableau le plus important à bien renseigner, et celui qui risque le
plus de devenir obsolète.** Listez chaque prestataire dont l'infrastructure
fait réellement transiter ou reposer vos données — pas une copie de la
liste de quelqu'un d'autre :

| Prestataire | Rôle | Données concernées | Localisation |
| --- | --- | --- | --- |
| `[Votre hébergeur]` | Hébergement serveur, base de données | Toutes les données du service, au repos | `[Localisation]` |
| `[Votre prestataire CDN/DNS, le cas échéant]` | CDN, DNS, pare-feu applicatif | IP, en-têtes, et — s'il fait proxy sur toute votre application, pas seulement le DNS — le contenu applicatif en transit | `[Localisation]` |
| `[Votre prestataire d'emailing, le cas échéant]` | Email transactionnel | Adresse email, contenu des messages | `[Localisation]` |

Si un prestataire se situe hors de la zone légale de vos utilisateurs (ex.
hors UE/EEE pour des utilisateurs européens), ajoutez une section sur les
transferts internationaux nommant la garantie réellement en place (clauses
contractuelles types, décision d'adéquation, certification) — ne revendiquez
pas une garantie dont vous n'avez pas vérifié qu'elle s'applique réellement
au prestataire concerné.

---

## 8. Cookies et traceurs

Si votre instance ne dépose aucun traceur au-delà de ce qui est strictement
nécessaire à son fonctionnement (cookies de session, protection CSRF, un
anti-bot type Turnstile/hCaptcha si vous en utilisez un), vous n'avez
probablement pas besoin de bandeau de consentement au regard du droit
UE/UK — mais c'est une affirmation factuelle sur ce que votre déploiement
dépose réellement, pas une valeur par défaut que vous pouvez présumer sans
vérifier votre propre configuration.

| Nom | Type | Finalité | Durée |
| --- | --- | --- | --- |
| `[nom du cookie de session]` | Cookie `httpOnly` | Accès à la session | `[Durée]` |
| `[nom du cookie de rafraîchissement]` | Cookie `httpOnly` | Renouvellement de session | `[Durée]` |

---

## 9. Sécurité des données

Vaut la peine d'être listé honnêtement, pas de façon aspirationnelle :
algorithme de hachage des mots de passe, chiffrement en transit, si les
sauvegardes sont chiffrées, et — spécifique à cette plateforme — le
mécanisme de chiffrement des flags et **ses limites énoncées** (il protège
contre une compromission limitée à la base de données, pas contre la
compromission de l'administrateur de l'instance ni un bug de permission).
Indiquez aussi honnêtement votre réalité opérationnelle : un VPS unique sans
redondance est une posture de sécurité très différente d'un déploiement
surveillé et redondant, et prétendre le contraire n'aide personne qui se fie
à ce document.

---

## 10. Vos droits

Si le RGPD ou un régime équivalent s'applique à vos utilisateurs, listez les
droits applicables (accès, rectification, effacement, portabilité,
opposition) et où adresser les demandes (`[VOTRE EMAIL VIE PRIVÉE]`). Nommez
les fonctionnalités en libre-service que votre instance a réellement
(export depuis les paramètres, suppression de compte en libre-service)
plutôt que de présumer qu'elles existent — ne renvoyez que vers celles
effectivement câblées dans votre déploiement.

---

## 11. Réclamations

Nommez l'autorité de contrôle réellement compétente pour vous — votre
autorité nationale de protection des données si vous êtes dans l'UE/EEE, ou
l'équivalent pertinent pour votre pays. Ne présumez pas d'une autorité d'un
pays donné sauf si vous y êtes effectivement établi.

---

## 12. Utilisateurs hors de votre juridiction principale

Si votre instance a une base d'utilisateurs internationale, indiquez
brièvement à quels autres régimes vous étendez volontairement des droits
équivalents (UK GDPR, LPD suisse, LGPD brésilienne, PIPEDA canadienne,
CCPA/CPRA américaine sont des choix courants à envisager), ou indiquez
clairement que vous ne vous engagez que sur le standard de votre juridiction
d'origine si c'est la réponse honnête.

---

## 13. Violations de données

Indiquez votre processus réel : qui vous notifieriez, dans quel délai, et
au titre de quel droit (la règle des 72 heures du RGPD envers votre autorité
de contrôle est la référence courante en UE, mais vérifiez ce qui
s'applique à vous).

---

## 14. Instances dérivées de la vôtre

Si votre code source est public, indiquez que vous n'êtes pas responsable
du traitement des données des instances supplémentaires que des tiers
déploient à partir de celui-ci, et que vous ne collectez aucune télémétrie
depuis elles, sauf si vous avez effectivement construit un mécanisme
opt-in à cet effet.

---

## 15. Modifications

Indiquez votre délai de préavis pour les changements substantiels (30 jours
est une pratique courante dans ce domaine) et où les changements sont
annoncés (email, bandeau in-app).

---

## 16. Historique des versions

| Version | Date | Modifications |
| --- | --- | --- |
| 0.1 | `[DATE]` | Version initiale |
