---
slug: terms
lang: fr
title: Conditions d'utilisation
description: Règles d'accès et d'utilisation de votre instance PwnForge, contenu, modération, suspension et responsabilité.
version: "0.1-template"
updated: "[DATE]"
---

> **⚠️ Gabarit, pas un conseil juridique.** Base de départ uniquement,
> adaptez chaque champ entre `[CROCHETS]` et chaque référence juridique à
> votre propre situation et juridiction avant de publier.

# Conditions d'utilisation

Indiquez clairement si le service est gratuit, payant, ou entre les deux —
ce seul fait détermine si les sections ci-dessous ont réellement une portée
(le droit de la consommation et les conditions de vente deviennent
pertinents dès qu'un paiement intervient).

---

## 1. Objet

Les présentes conditions régissent l'accès et l'utilisation de votre
instance PwnForge, exploitée par `[VOUS / VOTRE ORGANISATION]`, identifiée
dans les mentions légales.

---

## 2. Définitions

Gardez le vocabulaire de base, ça évite de le réécrire à chaque section :

| Terme | Définition |
| --- | --- |
| **Exploitant** | `[Vous / votre organisation]`, qui exploite cette instance. |
| **Service** | Cette instance de PwnForge, à l'adresse `[VOTRE DOMAINE]`. |
| **Compte**, **Workspace**, **Contenu utilisateur**, **Flag** | Mêmes définitions que celles utilisées dans l'interface de la plateforme. |

---

## 3. Acceptation

Indiquez que l'utilisation suppose l'acceptation des présentes conditions
via une action explicite à la création du compte, et qu'une personne
agissant pour une organisation déclare avoir l'autorité pour l'engager.

---

## 4. Nature du service

Indiquez honnêtement à quel stade se trouve votre instance (projet
personnel, hobby, service de production derrière une vraie équipe/société)
et ce qu'elle **ne** fournit **pas** : infrastructure d'attaque, exécution de
code arbitraire fourni par les utilisateurs, relais réseau. Ce cadrage est
ce qui permet de vous considérer comme hébergeur du contenu des
utilisateurs plutôt qu'éditeur de ce qu'ils organisent sur la plateforme.

---

## 5. Tarification

Si gratuit : dites-le clairement, et si vous pourriez introduire une
tarification plus tard, envisagez de vous engager sur un préavis et
l'absence de migration silencieuse vers une offre payante, comme le fait le
projet d'origine. Si déjà payant : ce gabarit ne couvre pas les conditions
de vente, le droit de rétractation du consommateur, ni la facturation — cela
nécessite un document séparé et sort du périmètre ici.

---

## 6. Portée internationale

Indiquez à partir de quel droit ces conditions sont rédigées (votre lieu
d'établissement) et qu'une règle impérative plus favorable du pays de
résidence d'un utilisateur reste prioritaire lorsqu'elle existe.

---

## 7. Éligibilité

Indiquez votre âge minimum (16 ans est courant pour ce type de plateforme,
mais vérifiez vos règles locales — certaines juridictions fixent 13 ans,
d'autres davantage) et toute autre condition d'accès pertinente pour vous
(ex. conformité aux sanctions internationales).

---

## 8. Compte utilisateur

Clauses standards à conserver : informations exactes à la création,
responsabilité de la sécurité des identifiants, un compte par personne,
suppression en libre-service disponible à tout moment.

---

## 9. Workspaces et équipes

Indiquez que l'accès suit des rôles résolus côté serveur, et que les
créateurs de workspace/équipe sont responsables de la composition de leur
espace et de la conformité du contenu qui y est publié.

---

## 10. Politique d'utilisation acceptable

**Conservez cette section quasiment telle quelle plutôt que de la
résumer** : c'est la partie qui fait le vrai travail juridique pour une
plateforme destinée à un public de sécurité offensive, et l'alléger serait
une mauvaise économie. N'ajustez que les lois citées pour refléter ce qui
est pertinent pour votre juridiction et celle probable de vos utilisateurs
(l'original cite le droit français, américain et britannique en exemple ;
ajoutez ou substituez le vôtre) :

- Pas d'intrusion non autorisée dans un système, ni d'hébergement de charges
  malveillantes prêtes à l'emploi ou d'infrastructure C2/phishing visant de
  vraies cibles.
- Pas de publication de données obtenues illégalement (fuites de données,
  vrais identifiants, données personnelles de tiers obtenues sans
  consentement).
- Pas de contenu manifestement illicite (pédopornographie, terrorisme,
  incitation à la haine, etc. — gardez cette liste, ne l'adoucissez pas).
- Pas d'attaque du service lui-même en dehors d'un cadre de signalement
  responsable défini (renvoyez vers votre section recherche de sécurité).
- Un utilisateur documentant un test d'intrusion déclare détenir une
  autorisation valide ; vous ne la vérifiez pas et n'êtes pas responsable de
  son absence.

---

## 11. Contenu utilisateur

Gardez la structure de base : les utilisateurs conservent la propriété, ils
ne vous accordent que la licence étroite nécessaire pour héberger et
afficher leur contenu (jamais une licence pour l'exploiter à d'autres fins
ou entraîner des modèles dessus), et la licence s'éteint quand le contenu
est supprimé, sous réserve du délai normal de purge des sauvegardes. Si vos
utilisateurs peuvent être liés par une clause de confidentialité (fréquent
dans un contexte pentest), gardez la clause rappelant que les paramètres de
visibilité privée ne les dispensent pas d'une obligation de confidentialité
à laquelle ils sont par ailleurs tenus.

---

## 12. Flags et contenu sensible

Renvoyez vers la description du chiffrement des flags et de ses limites
dans votre politique de confidentialité. Ajoutez une ligne précisant que les
utilisateurs ne doivent pas se fier à ce mécanisme au-delà de ce qu'il
protège réellement.

---

## 13. Intégrations tierces

Si vous activez des connecteurs de plateformes CTF, indiquez que la
connexion est à l'initiative de l'utilisateur, que les jetons sont chiffrés
et révocables, et que vous n'êtes pas responsable de la disponibilité des
plateformes tierces ni de l'évolution de leurs API.

---

## 14. Licence et marque

Indiquez que le cœur est sous licence AGPL v3, ne restreignant aucune des
libertés qu'elle accorde. Pour le point sur la marque : **n'incluez une
politique ici que si vous détenez réellement des droits sur le nom/la
marque de votre instance.** Si vous exploitez PwnForge en amont sans
modification, renvoyez vers la politique de marque du projet d'origine
plutôt que d'en réénoncer une sur laquelle vous n'avez aucune autorité.

---

## 15. Signalement et modération

Conservez l'échelle de gradation comme une structure juste et
proportionnée : avertissement → retrait de contenu → restriction de
fonctionnalité → suspension → résiliation, en appliquant la mesure la moins
restrictive qui résout le problème. Indiquez où adresser les signalements
(`[VOTRE EMAIL ABUSE]`).

---

## 16. Suspension et résiliation

C'est la section qu'il vaut le plus la peine de conserver proche de
l'original, car elle reflète un choix de conception délibéré et
défendable : **notifier qu'une mesure a été prise et son périmètre, sans
divulguer le raisonnement détaillé de détection**, parce que dans un
contexte de sécurité offensive, expliquer précisément comment vous avez
détecté quelque chose apprend à le contourner la fois suivante. Conservez :
- une voie de demande de réexamen motivée par un humain, sans délai de
  prescription ;
- les effets de la résiliation (données supprimées/anonymisées selon vos
  durées de conservation, une fenêtre d'export avant suppression définitive
  sauf en cas de contenu illicite ou de menace à la sécurité) ;
- un délai de préavis si vous cessez un jour le service entièrement.

---

## 17. Recherche de sécurité et divulgation

Indiquez votre périmètre de divulgation responsable (tests sur le propre
compte du chercheur ou une instance locale, pas d'exfiltration de données
d'autres utilisateurs, pas de dégradation de disponibilité, divulgation
coordonnée) et où adresser les signalements (`[VOTRE EMAIL SÉCURITÉ]`).
Offrir une récompense ou non dépend entièrement de vous et de vos moyens.

---

## 18. Disponibilité et modifications

Indiquez honnêtement votre posture réelle de disponibilité (un serveur
unique sans engagement de disponibilité est une affirmation très différente
d'une installation surveillée et redondante) et votre délai de préavis pour
retirer une fonctionnalité substantielle.

---

## 19. Absence de garantie / 20. Responsabilité

Clause de non-garantie « en l'état » standard, plus un plafond de
responsabilité adapté à votre situation — un plafond symbolique n'a de sens
que pour un service véritablement gratuit et non commercial ; ne recopiez
pas un plafond dérisoire si vous facturez de l'argent. Précisez
explicitement que certaines juridictions n'autorisent pas l'exclusion de
toute garantie implicite, et que le droit local impératif prime sur cette
clause lorsqu'il s'applique.

---

## 21. Données personnelles

Renvoyez vers votre politique de confidentialité comme faisant partie
intégrante des présentes conditions.

---

## 22. Instances dérivées de la vôtre

Si votre code source est public et qu'un tiers déploie une instance
supplémentaire à partir de celui-ci, indiquez que les présentes conditions
ne s'appliquent pas à cette instance, que son exploitant en porte l'entière
responsabilité, et qu'il doit se conformer à l'article 13 de l'AGPL v3
(mise à disposition de son code source modifié) s'il a modifié quoi que ce
soit.

---

## 23. Modifications des présentes conditions

Indiquez votre délai de préavis pour les changements substantiels et que la
poursuite de l'utilisation après ce délai vaut acceptation, tandis que les
changements imposés par la loi peuvent prendre effet immédiatement.

---

## 24. Divers

Clauses standards à conserver : intégralité de l'accord, divisibilité,
absence de renonciation, conditions de cession, force majeure, langue
faisant foi si vous publiez en plusieurs langues, et le fait que vos
journaux techniques/d'audit sont admissibles comme preuve entre les parties.

---

## 25. Droit applicable et litiges

Indiquez votre droit applicable (votre lieu d'établissement) et où les
litiges sont portés, en précisant qu'un droit impératif du pays de
résidence d'un utilisateur peut lui garantir la possibilité d'agir
localement, ce que cette clause ne peut supprimer.

---

## 26. Historique des versions

| Version | Date | Modifications |
| --- | --- | --- |
| 0.1 | `[DATE]` | Version initiale |
