# Guide utilisateur de LockNote 2.1

[English](../en/user-guide.md) | [README](../../README.fr.md)

LockNote est un bloc-notes chiffré pour Windows qui réunit le programme et vos
notes dans un seul fichier `.exe`. Rien à installer, aucun compte, rien
n'est envoyé dans un cloud. Depuis la 2.0.0, toutes les fonctions sont
gratuites ; il n'y a plus d'édition Pro.

Ce guide décrit toutes les fonctions de LockNote 2.1. Les noms de menus et
de boutons sont ceux de l'interface en français (Fichier > Paramètres... >
Préférences... > Langue : Français). Les fenêtres affichées avant le
déchiffrement des notes suivent la langue de Windows (voir
[section 1](#1-premier-lancement-et-mot-de-passe)) : ce guide les cite telles
qu'elles s'affichent sous un Windows en français. Les détails techniques
ajoutés à certains messages d'erreur restent en anglais.

## Sommaire

1. [Premier lancement et mot de passe](#1-premier-lancement-et-mot-de-passe)
2. [Où sont stockées vos notes](#2-où-sont-stockées-vos-notes)
3. [Déverrouillage au démarrage](#3-déverrouillage-au-démarrage)
4. [Verrouillage et écran verrouillé](#4-verrouillage-et-écran-verrouillé)
5. [La fenêtre principale](#5-la-fenêtre-principale)
6. [Enregistrer et fermer](#6-enregistrer-et-fermer)
7. [Onglets](#7-onglets)
8. [Fonctions de l'éditeur](#8-fonctions-de-léditeur)
9. [Coffre](#9-coffre)
10. [Prompts](#10-prompts)
11. [Gestionnaire TOTP](#11-gestionnaire-totp)
12. [Authentification à deux facteurs au déverrouillage](#12-authentification-à-deux-facteurs-au-déverrouillage)
13. [Pièces jointes](#13-pièces-jointes)
14. [Stéganographie](#14-stéganographie)
15. [Clés SSH et ssh-agent](#15-clés-ssh-et-ssh-agent)
16. [Générateur de mots de passe](#16-générateur-de-mots-de-passe)
17. [Verrouillage auto, sauvegarde auto et effacement du presse-papiers](#17-verrouillage-auto-sauvegarde-auto-et-effacement-du-presse-papiers)
18. [Raccourci global et zone de notification](#18-raccourci-global-et-zone-de-notification)
19. [Langue et thème](#19-langue-et-thème)
20. [Sauvegardes de sécurité](#20-sauvegardes-de-sécurité)
21. [Sauvegarde complète (.lnbak)](#21-sauvegarde-complète-lnbak)
22. [Importer depuis un autre LockNote](#22-importer-depuis-un-autre-locknote)
23. [Fichiers temporaires](#23-fichiers-temporaires)
24. [Mises à jour](#24-mises-à-jour)
25. [Migrer depuis LockNote 1.2.x](#25-migrer-depuis-locknote-12x)
26. [Référence des paramètres](#26-référence-des-paramètres)
27. [Raccourcis clavier](#27-raccourcis-clavier)
28. [Ce qui est écrit en clair sur le disque](#28-ce-qui-est-écrit-en-clair-sur-le-disque)

---

## 1. Premier lancement et mot de passe

**Ce que ça fait.** À la première ouverture d'un `LockNote.exe` qui ne contient
aucune note, LockNote vous demande le mot de passe qui va les chiffrer.

**Mode d'emploi.**

1. Double-cliquez sur `LockNote.exe`.
2. La fenêtre **Créer un mot de passe** s'ouvre : « Bienvenue sur LockNote ».
   Tapez votre mot de passe dans **Mot de passe (min. 6 caractères) :**, puis
   le même dans **Confirmer :**.
3. Cliquez sur **Créer** (ou appuyez sur Entrée).

**Options et valeurs par défaut.**

- Longueur minimale : 6 caractères pour une nouvelle note. Elle se change
  ensuite dans Fichier > Paramètres... > Sécurité... (voir la
  [section 26](#26-référence-des-paramètres)).
- La jauge de force sous le mot de passe est indicative. Elle compte un point
  pour chacun de ces critères : au moins 8 caractères, au moins 12, au moins
  16, une minuscule, une majuscule, un chiffre, un autre caractère. 0 à 1 point
  : **Faible**, 2 à 3 : **Moyen**, 4 à 5 : **Fort**, 6 à 7 : **Très fort**. Seule la longueur minimale est imposée.

**Attention.**

- Les erreurs s'affichent dans la fenêtre : « Le mot de passe est trop
  court. » ou « Les mots de passe ne correspondent pas. » (les deux saisies
  diffèrent).
- **Annuler** ferme LockNote sans rien créer.
- Un mot de passe oublié ne se récupère pas. Vos notes sont chiffrées avec lui
  (AES-256-CBC + HMAC-SHA256, clé dérivée par Argon2id, 64 Mio,
  3 itérations, 4 voies).
- Cette fenêtre, celle du déverrouillage au démarrage et les messages de
  démarrage sont dans la langue de Windows quand c'est le français, l'anglais,
  l'espagnol ou l'allemand, en anglais sinon : la langue choisie est
  enregistrée dans les notes chiffrées et n'est connue qu'une fois celles-ci
  déchiffrées. Ensuite, c'est votre choix qui s'applique.
- Une nouvelle note commence dans cette même langue : son réglage de langue,
  le nom de son premier onglet et ses catégories du coffre. Vous pouvez changer
  de langue ensuite ([section 19](#19-langue-et-thème)) ; les noms déjà créés
  ne changent pas.

## 2. Où sont stockées vos notes

**Ce que ça fait.** Vos notes sont stockées à la fin du `.exe` lui-même.
Copiez l'exe (sur une clé USB, un autre ordinateur) : vos notes le suivent.

**Comment se passe un enregistrement.** Un programme en cours d'exécution ne
peut pas réécrire son propre fichier. Chaque enregistrement va donc d'abord
dans un fichier d'attente de `%LOCALAPPDATA%\LockNote`, nommé `<hash>.tmp`,
accompagné d'un fichier compagnon `<hash>.base`. À la fermeture de LockNote, un
petit programme d'aide intégré à l'exe recopie le `.tmp` dans l'exe. Cela marche
aussi quand l'exe est sur un autre lecteur (clé USB, second disque, partage
réseau).

**Attention.**

- **Tant que LockNote n'est pas fermé, le fichier `.tmp` est la seule copie de
  votre dernier enregistrement. Ne le supprimez jamais à la main.** Fichier >
  Paramètres... > Fichiers temporaires... affiche ces fichiers et ce qu'est
  chacun d'eux ([section 23](#23-fichiers-temporaires)).
- Ne copiez l'exe qu'une fois LockNote fermé. Une copie faite pendant qu'il
  tourne ne contient pas le dernier enregistrement.
- Le `.tmp` contient les mêmes données chiffrées que l'exe : rien n'est écrit
  en clair.
- Un seul LockNote tourne à la fois. En lancer un second ramène au premier plan
  la fenêtre de celui qui tourne.

**Deux versions de vos notes.** Si, au démarrage, l'exe contient d'autres notes
que celles sur lesquelles reposait l'enregistrement en attente (l'exe a été
remplacé ou écrit ailleurs pendant qu'un enregistrement attendait), LockNote
pose la question avant tout mot de passe : « LockNote - Deux versions de vos
notes ». Le message donne les deux dates (UTC) et le dossier
où va la version que vous ne gardez pas :

- **Oui** : garder l'enregistrement en attente.
- **Non** : garder les notes du fichier programme.
- **Annuler** : quitter sans rien changer ; la question revient au prochain
  démarrage.

La version que vous ne gardez pas n'est jamais supprimée : elle est déplacée
dans `%LOCALAPPDATA%\LockNote\conflicts\` sous forme d'un `.exe` LockNote
complet, que vous pouvez ouvrir avec Fichier > Importer depuis un autre
LockNote... et le mot de passe qu'il avait.

Si LockNote ne peut pas lire le fichier programme ou un enregistrement en
attente, il affiche un message qui nomme le fichier et se ferme sans rien
modifier.

## 3. Déverrouillage au démarrage

**Ce que ça fait.** Quand l'exe contient déjà des notes, LockNote demande le mot
de passe avant de les afficher.

**Mode d'emploi.**

1. Lancez LockNote. La fenêtre **Déverrouiller** demande : « Entrez votre mot
   de passe pour déverrouiller vos notes. »
2. Tapez le mot de passe. Le bouton en forme d'œil affiche ou masque la saisie.
   « Verr. Maj activé » s'affiche quand la touche Verr. Maj est active et que
   le champ a le focus.
3. Appuyez sur Entrée ou cliquez sur **Déverrouiller**. « Déchiffrement... »
   s'affiche pendant le calcul de la clé.
4. Si l'authentification à deux facteurs est active, le code à 6 chiffres est
   demandé ensuite ([section 12](#12-authentification-à-deux-facteurs-au-déverrouillage)).

**Limites.**

- 5 tentatives. Après un mauvais mot de passe : « Mot de passe incorrect.
  (N/5) ». À partir de la 2e erreur, il faut attendre avant la suivante : 1 s
  après la 2e, 2 s après la 3e, 4 s après la 4e (« Nouvelle tentative dans
  Ns… »). Après la 5e : « Nombre maximum de tentatives atteint. Fermeture. » et
  LockNote se ferme.
- **Annuler** ou Échap ferme LockNote.

## 4. Verrouillage et écran verrouillé

**Ce que ça fait.** Le verrouillage cache vos notes et efface de la mémoire le
mot de passe et le contenu déchiffré, jusqu'à ce que vous retapiez le mot de
passe.

**Où c'est.**

- Le bouton **Verrouiller** (cadenas) à droite de la barre d'onglets.
- **Ctrl+L**.
- Automatiquement après une période d'inactivité (verrouillage auto,
  [section 17](#17-verrouillage-auto-sauvegarde-auto-et-effacement-du-presse-papiers)).

**Ce qui se passe au verrouillage.**

1. Les clés SSH que ce fichier a chargées dans ssh-agent en sont retirées.
2. Les modifications non enregistrées sont enregistrées.
3. L'onglet actif et la ligne du curseur sont mémorisés.
4. L'éditeur, le coffre, les prompts et les pièces jointes sont vidés de la
   mémoire.
5. La fenêtre affiche l'écran verrouillé : « LockNote est verrouillé », un
   champ mot de passe avec un bouton œil, et **Déverrouiller**.

**Déverrouiller.** Tapez le mot de passe et appuyez sur Entrée ou cliquez sur
**Déverrouiller**. Vous revenez au même onglet et à la même ligne.

**Limites et précautions.**

- 5 tentatives, avec les mêmes attentes qu'au démarrage (1 s, 2 s, 4 s) :
  « Mot de passe incorrect. (N/5) », « Nouvelle tentative dans Ns… ». Après la
  5e erreur : « Nombre maximum de tentatives atteint. Fermeture. » et LockNote
  se ferme.
- L'écran verrouillé ne demande que le mot de passe, pas le code à deux
  facteurs.
- Pendant le verrouillage, les raccourcis clavier ne font rien et les fichiers
  déposés sur la fenêtre sont ignorés.
- Fermer LockNote pendant qu'il est verrouillé quitte sans enregistrer :
  l'enregistrement a été fait au verrouillage.

## 5. La fenêtre principale

**Barre de menus.** Fichier, Édition, Affichage, Onglet, Outils, Aide.

| Menu | Commandes |
|---|---|
| Fichier | Enregistrer, Imprimer..., Exporter..., Exporter une sauvegarde complète..., Importer une sauvegarde complète..., Importer depuis un autre LockNote..., Sauvegardes de sécurité..., Paramètres... (Préférences..., Sécurité..., Fichiers temporaires...), Quitter |
| Édition | Couper, Copier, Coller, Coller en texte brut, Rechercher..., Remplacer..., Rechercher dans tous les onglets..., Aller à la ligne..., Aller au titre..., Dupliquer la ligne, Supprimer la ligne, Lignes (Trier par ordre croissant, Trier par ordre décroissant, Supprimer les lignes en double, Joindre les lignes, Supprimer les espaces de fin), Convertir la casse (MAJUSCULES, minuscules, Première lettre des mots en majuscule), Tout sélectionner, Insérer un horodatage |
| Affichage | Toujours au premier plan, Retour à la ligne, Numéros de ligne |
| Onglet | Nouvel onglet, Fermer l'onglet, Renommer l'onglet..., Onglet suivant, Onglet précédent, Comparer avec le dernier enregistrement... |
| Outils | Gestionnaire de coffre..., Prompts, Gestionnaire TOTP, Pièces jointes, Générateur de mots de passe..., Stéganographie (Cacher dans une image..., Extraire depuis une image...), Clés SSH (Générer une clé SSH..., Charger dans ssh-agent...) |
| Aide | Vérifier les mises à jour..., À propos de LockNote... |

**Barre d'onglets.** Vos onglets, le bouton **+** (nouvel onglet) et, à droite,
les boutons **Prompts**, **Coffre**, **TOTP**, **Fichiers** et **Verrouiller**.
Chaque bouton ouvre ou ferme son panneau latéral ; il est mis en évidence tant
que son panneau est ouvert.

**Panneaux latéraux.** Coffre, Prompts, TOTP et Pièces jointes s'ouvrent à
droite de l'éditeur, un seul à la fois. Le panneau Coffre est ouvert au
démarrage. Faites glisser la fine barre entre l'éditeur et le panneau pour le
redimensionner. La largeur est conservée pendant la session et enregistrée avec
la note ; elle ne fait pas partie d'une sauvegarde complète.

**Barre d'état.**

- À gauche : mots, caractères et lignes de l'onglet actif.
- Au milieu : le chemin complet du fichier LockNote. Un clic le copie dans le
  presse-papiers (« Chemin copié »).
- À droite : la position du curseur (« Ligne 3, col. 5 », voir section 8),
  puis un point avec **Enregistré** ou **Modifié**, puis **Chiffré**, puis
  l'encodage (UTF-8).

**Titre de la fenêtre.** « LockNote », ou « LockNote * » quand des
modifications ne sont pas enregistrées.

**Position de la fenêtre.** La taille, la position et l'état agrandi sont
rétablis au démarrage suivant (enregistrés avec la note à la fermeture). Si la
position enregistrée n'est plus sur un écran (moniteur débranché), la fenêtre
s'ouvre à une place par défaut.

**À propos.** Aide > À propos de LockNote... affiche la version, le chiffrement
utilisé et l'adresse du projet.

## 6. Enregistrer et fermer

**Enregistrer.** Fichier > Enregistrer, ou **Ctrl+S**. Le raccourci se change
dans Fichier > Paramètres... > Préférences... > **Raccourci de sauvegarde :**
(cliquez dans le champ puis appuyez sur la nouvelle combinaison : Ctrl, Maj
et/ou Alt avec une lettre A-Z ou F1-F24 ; au moins un modificateur est
obligatoire). Le nouveau raccourci marche tout de suite ; le menu l'affiche à
partir du démarrage suivant (la palette de commandes l'affiche tout de suite).

**À la fermeture.** Fichier > Quitter, Ctrl+Q, Alt+F4 ou le bouton de fermeture
de la fenêtre. Ce qui arrive aux modifications non enregistrées dépend de
Fichier > Paramètres... > Préférences... > **À la fermeture :**

- **Demander** (par défaut) : « Modifications non enregistrées. Enregistrer
  avant de fermer ? » **Enregistrer** enregistre, **Ne pas enregistrer** ferme
  sans enregistrer, **Annuler** laisse LockNote ouvert. **Se souvenir de mon
  choix** avec Enregistrer règle À la fermeture sur Toujours enregistrer : la
  question n'est plus posée. Avec Ne pas enregistrer, le choix ne peut pas être
  retenu : les paramètres sont rangés dans la note, qui n'est pas enregistrée ;
  LockNote le signale (choisissez plutôt Ne jamais enregistrer dans les
  Préférences, puis enregistrez). Annuler ne change jamais le réglage.
- **Toujours enregistrer** : enregistre sans demander.
- **Ne jamais enregistrer** : ferme sans enregistrer.

Sauf si vous avez choisi de ne pas enregistrer, LockNote enregistre une
dernière fois à la fermeture, ce qui conserve aussi la position de la fenêtre
et les paramètres.

**Sauvegarde auto.** Voir la [section 17](#17-verrouillage-auto-sauvegarde-auto-et-effacement-du-presse-papiers).

**Fermeture de session ou arrêt de Windows.** LockNote ne peut alors rien
demander. Il enregistre les notes, comme si vous aviez répondu Enregistrer,
sauf si À la fermeture vaut **Ne jamais enregistrer** (les modifications non
enregistrées sont alors perdues, comme pour une fermeture normale) ; un
LockNote verrouillé a déjà enregistré au verrouillage. La même règle
s'applique pendant que les notes sont masquées après un verrouillage qui n'a
pas encore pu enregistrer : elles sont enregistrées avec leurs
modifications, sauf si À la fermeture vaut **Ne jamais enregistrer**, votre
choix : ces modifications non enregistrées sont alors perdues. Puis il se ferme comme
d'habitude (clés SSH retirées de ssh-agent, mise à jour ou enregistrement en
attente appliqué). Si l'enregistrement échoue, Windows affiche « LockNote n'a
pas pu enregistrer vos notes (…) » sur son écran d'arrêt : annulez, puis
enregistrez ou copiez votre texte. Si les notes n'avaient pas de
modifications non enregistrées, seule la position de la fenêtre est en jeu :
un échec de l'enregistrement n'empêche pas l'arrêt. Si vous étiez en train de
fermer LockNote et aviez déjà répondu **Ne pas enregistrer** (ou **Fermer
sans enregistrer** après un enregistrement qui a échoué), la fermeture de
session respecte cette réponse et n'enregistre rien, même si un message de
cette fermeture est encore affiché ; une question de fermeture à laquelle
vous n'avez pas encore répondu suit la règle ci-dessus. La liste des clés SSH chargées par ce
fichier n'est pas réenregistrée à ce moment : au déverrouillage suivant,
LockNote retire à nouveau ces clés de ssh-agent, donc une clé ajoutée à la
main entre-temps est retirée aussi ; ajoutez-la de nouveau si besoin.

**Attention.** Si un enregistrement échoue, un message « Erreur de
sauvegarde » en donne la raison. Vos modifications sont toujours dans la
fenêtre : réessayez avant de fermer. Quand un enregistrement échoue pendant la
fermeture (**Enregistrer** à la question, Toujours enregistrer, ou le dernier
enregistrement), LockNote affiche « LockNote n'a pas pu enregistrer : … »
avec la raison. **Garder LockNote ouvert** (par défaut, aussi Entrée, Échap ou
la croix de la fenêtre) garde votre texte dans la fenêtre ; **Fermer sans
enregistrer** ferme et les modifications non enregistrées sont perdues. Les
clés SSH chargées dans ssh-agent par ce fichier ont déjà été retirées à ce
moment : rechargez-les si vous gardez LockNote ouvert.

**Caractère invalide.** Un autre programme peut taper ou coller la moitié
d'un emoji ou d'un symbole dans l'éditeur. LockNote ne peut pas enregistrer
un tel caractère. Un enregistrement, un changement d'onglet, un export, une
sauvegarde et les commandes qui réécrivent le texte s'arrêtent alors avec un
message « Caractère invalide » qui donne sa ligne, et rien n'est écrit. Le
caractère peut apparaître comme une case vide, ou pas du tout. Après un
changement d'onglet, un export, une sauvegarde ou une commande refusés,
LockNote le sélectionne : appuyez sur Suppr, puis réessayez. Un
enregistrement refusé (Ctrl+S, enregistrement automatique) donne sa ligne
sans le sélectionner.

Les champs de saisie (mots de passe, secrets du coffre, prompts, noms
d'onglet et de clé, Rechercher et remplacer) refusent un tel caractère de
la même façon : l'action s'arrête avec « Ce champ contenait un caractère
invalide », rien n'est enregistré, comparé ni compté comme un mauvais mot
de passe, et la fenêtre reste ouverte. Un champ de mot de passe est vidé :
saisissez à nouveau le mot de passe. Dans un champ visible, le caractère
est sélectionné : appuyez sur Suppr. Les champs de recherche l'affichent
comme un signe de remplacement et continuent de fonctionner. Avant ce
correctif, un mot de passe saisi avec la moitié d'un emoji était
enregistré comme le texte `Decoding error` : si une note, une clé SSH, une
image de stéganographie ou une sauvegarde a été protégée ainsi, tapez
`Decoding error` pour l'ouvrir, puis changez son mot de passe.

## 7. Onglets

**Ce que ça fait.** Un fichier LockNote contient jusqu'à 20 onglets, chacun avec
son texte, son nom et une couleur facultative.

**Mode d'emploi.**

- **Nouvel onglet** : le bouton **+**, Onglet > Nouvel onglet ou **Ctrl+T**.
  Les nouveaux onglets s'appellent « Note N » (« Nota N » en espagnol,
  « Notiz N » en allemand), N étant le nombre d'onglets ; le nom reste le même
  si vous changez de langue ensuite. À 20 onglets : « Nombre maximum
  d’onglets atteint (20). »
- **Changer d'onglet** : cliquez sur l'onglet, ou **Ctrl+Tab** /
  **Ctrl+Maj+Tab** (Onglet > Onglet suivant / Onglet précédent).
- **Fermer** : la croix de l'onglet, Onglet > Fermer l'onglet, **Ctrl+W**, ou
  clic droit > Fermer l’onglet. Un onglet qui contient du texte demande
  d'abord : « Fermer l’onglet « nom » ? Son contenu sera perdu. » Le dernier
  onglet ne peut pas être fermé.
- **Renommer** : Onglet > Renommer l'onglet... (**F2**), ou clic droit sur l'onglet >
  Renommer l’onglet.... Le nom ne peut pas être vide.
- **Couleur** : clic droit sur l'onglet > Couleur > une couleur (Bleu, Vert,
  Rouge, Orange, Violet, Jaune, Rose, Gris), ou Aucune couleur. L'onglet affiche une pastille de cette couleur.
- **Réordonner** : appuyez sur un onglet, faites-le glisser sur un autre onglet
  et relâchez. Une marque indique où il va se placer.
- **Onglets masqués** : quand les onglets ne tiennent pas, un bouton à liste
  apparaît à droite de la barre. Il liste tous les onglets ; choisissez-en un
  pour y aller.

L'onglet actif et la ligne du curseur sont rétablis après un verrouillage et au
démarrage suivant.

## 8. Fonctions de l'éditeur

### Commandes d'édition

| Commande | Où | Raccourci |
|---|---|---|
| Couper, Copier, Coller | menu Édition, clic droit dans l'éditeur | Ctrl+X, Ctrl+C, Ctrl+V |
| Coller en texte brut | menu Édition, clic droit | Ctrl+Maj+V |
| Tout sélectionner | menu Édition, clic droit | Ctrl+A |
| Annuler, Rétablir | clavier seulement | Ctrl+Z, Ctrl+Y |
| Dupliquer la ligne | menu Édition | Ctrl+D |
| Supprimer la ligne | menu Édition | Ctrl+Maj+K |
| Déplacer les lignes vers le haut / le bas | clavier seulement | Alt+Haut, Alt+Bas |
| Trier, supprimer les doublons, joindre, supprimer les espaces de fin | Édition > Lignes | aucun |
| MAJUSCULES, minuscules | Édition > Convertir la casse | Ctrl+Maj+U, Ctrl+U |
| Première lettre des mots en majuscule | Édition > Convertir la casse | aucun |
| Indenter / désindenter des lignes | clavier seulement | Tab, Maj+Tab |
| Insérer un horodatage | menu Édition | F5 |
| Puce sur la ligne courante | clavier seulement | Ctrl+. (point) |
| Case à cocher sur la ligne courante | clavier seulement | Ctrl+Entrée |
| Palette de commandes | clavier seulement | Ctrl+Maj+P |
| Zoom avant, zoom arrière, taille d'origine | clavier et souris | Ctrl+molette, Ctrl+=, Ctrl+-, Ctrl+0 |

- **Insérer un horodatage** insère la date et l'heure au format
  `AAAA-MM-JJ HH:MM:SS`, à l'heure locale de l'ordinateur (sans fuseau
  horaire affiché). Au retour à l'heure d'hiver, la même heure passe deux
  fois : deux horodatages de cette heure-là peuvent sembler identiques ou
  dans le désordre.
- **Puces** : Ctrl+. ajoute « • » en début de ligne, ou l'enlève. Sur une ligne
  à puce, Entrée commence une nouvelle ligne à puce ; Entrée sur une ligne à
  puce vide termine la liste. Cela ne marche que si la puce commence la ligne
  (sans indentation devant).
- **Double-clic** : sélectionne tout le texte compris entre deux espaces ; un
  mot de passe comme `Pass@word1!` est donc sélectionné en entier. Rien n'est
  copié : la copie se fait toujours par Ctrl+C.
- **Tout sélectionner** (Ctrl+A) sélectionne tout le texte, émojis compris.
- **Surlignage des occurrences** : quand vous sélectionnez un mot (2 à 256
  caractères, sur une ligne), ses autres occurrences dans l'onglet sont
  surlignées.
- **Liens** : un texte qui commence par `http://` ou `https://` s'affiche comme
  un lien ; un clic l'ouvre dans le navigateur.
- **Numéros de ligne** : affichés dans une marge à gauche du texte.

### Annulation et onglets

Tous les onglets partagent un seul éditeur, donc un seul historique
d'annulation, et il est vidé chaque fois que l'éditeur affiche un autre onglet
(clic sur un onglet, menu des onglets en surnombre, Ctrl+Tab / Ctrl+Maj+Tab,
nouvel onglet, onglet fermé, extraction de stéganographie ouverte dans un
nouvel onglet). Ctrl+Z / Ctrl+Y ne peuvent donc jamais amener le texte d'un
onglet dans un autre. Juste après un changement d'onglet, il n'y a rien à
annuler ni à rétablir dans cet onglet ; les modifications suivantes
s'annulent normalement, jusqu'au texte affiché à l'arrivée. En revenant sur un
onglet, vous retrouvez son dernier texte, mais ses modifications antérieures ne
peuvent plus être annulées.

Un changement de police (zoom, Préférences), un changement de thème et le
surlignage des occurrences du mot sélectionné suspendent l'historique
d'annulation le temps de s'appliquer, pour ne jamais devenir une étape
d'annulation. Si Windows refuse de le rétablir, un message l'indique :
« L'historique d'annulation n'a pas pu être rétabli : les prochaines
modifications pourraient ne pas pouvoir être annulées. Enregistrez votre
note. » Il s'affiche une seule fois, une fois le zoom ou le surlignage
terminé (après le déverrouillage, ou à la fermeture de la boîte de dialogue
ouverte, si LockNote est verrouillé ou qu'une autre boîte est ouverte, la
fenêtre Rechercher et remplacer comprise), et de nouveau seulement si
l'historique a refonctionné entre-temps. Si LockNote se verrouille pendant
que le message est affiché, il se ferme et revient après le déverrouillage.

### Indenter et désindenter (Tab, Maj+Tab)

- **Tab avec une sélection sur deux lignes ou plus** insère une tabulation au
  début de chaque ligne touchée, lignes vides comprises. **Maj+Tab** enlève
  une tabulation en début de chaque ligne touchée ou, s'il n'y en a pas,
  jusqu'à 4 espaces. Ensuite, la sélection couvre entièrement les lignes
  touchées : un nouvel appui sur Tab ou Maj+Tab agit sur les mêmes lignes.
- Une ligne où la sélection se termine en colonne 0 n'est pas touchée. Une
  ligne entière sélectionnée avec son saut de ligne (par exemple Début puis
  Maj+Bas) est indentée au lieu d'être remplacée par une tabulation ; ensuite
  la sélection se termine toujours en colonne 0 de la ligne suivante.
- **Tab sans sélection ou avec une sélection sur une seule ligne** insère une
  tabulation à la place de la sélection, comme avant.
- **Maj+Tab sans sélection ou avec une sélection sur une seule ligne** enlève
  un niveau (une tabulation, sinon jusqu'à 4 espaces) à la ligne courante ; le
  curseur recule avec le texte. S'il n'y a rien à enlever, rien ne se passe et
  la sélection reste telle quelle.
- Chaque indentation ou désindentation s'annule en un seul Ctrl+Z. Ctrl+Tab et
  Ctrl+Maj+Tab changent toujours d'onglet.
- Limites : une ligne à puce indentée (une tabulation puis « • a ») n'est plus
  vue comme une puce : Entrée ne continue pas la liste et Ctrl+. ajoute une
  deuxième puce. Après un Maj+Tab qui a enlevé de l'indentation, une sélection
  faite à l'envers devient une sélection à l'endroit.

### Listes numérotées et cases à cocher

- **Listes numérotées** : Entrée sur une ligne qui commence par « N. » ou
  « N) » (N = 1 à 9 chiffres, éventuellement indenté par des espaces ou des
  tabulations) commence la ligne suivante par N+1, avec la même ponctuation et
  la même indentation (« 007. » continue par « 8. »). Entrée sur un élément
  vide (« 3. » sans rien après) termine la liste : la ligne est vidée et
  aucune ligne n'est ajoutée.
- **Cases à cocher** : « [ ] » (non cochée), « [x] » ou « [X] » (cochée) en
  début de ligne, éventuellement après une indentation et/ou une puce « • » ou
  « - ». Entrée sur une ligne à case continue par une case « [ ] » non cochée,
  avec la même indentation et la même puce ; Entrée sur un élément vide
  termine la liste.
- **Ctrl+Entrée** coche ou décoche la case de la ligne courante (« [ ] » et
  « [x] »). Sur une ligne sans case, il insère « [ ] » après l'indentation et la
  puce (sur « 1. tâche », cela donne « [ ] 1. tâche »).
- Avec une sélection, Entrée se comporte normalement. Entrée au milieu d'une
  ligne de liste emmène la fin de la ligne dans le nouvel élément. Les listes
  restent du texte brut : rien d'autre n'est enregistré.

### Opérations sur les lignes

- **Alt+Haut / Alt+Bas** déplacent la ligne courante, ou toutes les lignes
  touchées par la sélection, d'une ligne vers le haut ou vers le bas. La
  sélection suit les lignes et garde son sens ; le curseur garde sa colonne.
  Rien ne se passe sur la première ligne (Alt+Haut) ou la dernière (Alt+Bas).
  Maintenir les touches répète le déplacement, une étape d'annulation par
  déplacement. Ces raccourcis ne figurent dans aucun menu.
- **Édition > Lignes** :
  - **Trier par ordre croissant / décroissant** : selon les paramètres
    régionaux de Windows, sans tenir compte de la casse ni des accents (comme
    l'arborescence du coffre) ; les lignes égales gardent leur ordre ; les
    lignes vides viennent en premier dans l'ordre croissant. Si la comparaison
    du système échoue, le texte reste inchangé et un message donne la raison.
  - **Supprimer les lignes en double** : égalité exacte (la casse et les
    espaces comptent), la première occurrence est gardée.
  - **Joindre les lignes** : une seule ligne, un espace entre les lignes
    jointes, dont les blancs de début sont enlevés ; la première ligne garde
    son indentation.
  - **Supprimer les espaces de fin** : enlève espaces, tabulations et autres
    blancs en fin de chaque ligne.
- Portée : les lignes touchées par la sélection quand elle s'étend sur deux
  lignes ou plus, sinon tout l'onglet (sans la ligne vide qui suit un saut de
  ligne final : une note qui se termine par un saut de ligne le garde). Une
  sélection qui se termine tout au début d'une ligne n'inclut pas cette ligne.
- Ensuite, les lignes obtenues sont sélectionnées : vous pouvez enchaîner
  (trier, puis supprimer les doublons). Chaque opération s'annule en un seul
  Ctrl+Z ; une opération qui ne change rien n'ajoute pas d'étape d'annulation
  et ne marque pas la note comme modifiée.

### Convertir la casse

Édition > Convertir la casse : **MAJUSCULES** (Ctrl+Maj+U), **minuscules**
(Ctrl+U), **Première lettre des mots en majuscule** (menu seulement : dans
chaque mot, la première lettre passe en majuscule et le reste en minuscules).

- La conversion porte sur la sélection ou, sans sélection, sur le mot sous le
  curseur (le texte que sélectionne un double-clic, ponctuation comprise :
  `user@mail.com` est un seul mot). Sur un blanc ou dans un onglet vide, rien
  ne se passe.
- Pour la première lettre en majuscule, les mots ne sont séparés que par les
  blancs : « don't » -> « Don't », « jean-PIERRE » -> « Jean-pierre »,
  « (bonjour) » -> « (Bonjour) », « 42abc » -> « 42Abc ».
- Les accents et les autres écritures sont gérés, et la longueur peut changer
  (« straße » -> « STRASSE »). Le texte converti est ensuite sélectionné, pour
  enchaîner les conversions.
- Une conversion s'annule en un seul Ctrl+Z et marque l'onglet comme modifié ;
  un texte déjà dans la casse demandée n'est pas touché (pas d'étape
  d'annulation). Si le raccourci de sauvegarde est Ctrl+U, Ctrl+U enregistre.

### Palette de commandes

Ctrl+Maj+P ouvre la palette de commandes (pas d'entrée de menu ; pas quand
LockNote est verrouillé).

- Un champ de filtre au-dessus d'une liste. Filtre vide, la liste montre
  toutes les commandes des menus (Fichier, Édition, Affichage, Onglet, Outils,
  Aide) avec leur raccourci à droite, puis « Onglet : <nom> » pour chaque
  onglet, « Coffre : <nom> » pour chaque secret et « Prompt : <nom> » pour
  chaque prompt. Deux secrets de même nom affichent leur catégorie :
  « Coffre : admin (Travail) ».
- Le filtre ignore la casse et les espaces ; les lettres tapées doivent
  apparaître dans l'ordre, pas forcément côte à côte (« dl » trouve
  « Dupliquer la ligne »). Les libellés qui commencent par le texte tapé
  viennent en premier. Les lettres accentuées doivent être tapées telles
  quelles (« e » ne trouve pas « é »).
- Haut/Bas déplacent la sélection d'une ligne, Page précédente/Page suivante
  d'une page visible (en s'arrêtant à la première ou à la dernière ligne).
  Entrée exécute la ligne sélectionnée, comme un double-clic sur une ligne.
  Échap ferme la palette et revient à l'éditeur.
- Une commande fait ce que fait son entrée de menu. Une ligne d'onglet affiche
  cet onglet. Une ligne de coffre copie le secret comme un clic dans le
  panneau Coffre (le panneau s'ouvre, « Copié : <nom> », presse-papiers effacé
  après le délai réglé) ; la valeur n'est jamais affichée. Une ligne de prompt
  insère le prompt dans la note (voir section 10), même panneau Prompts
  fermé.
- Si la note se verrouille pendant que la palette est ouverte, ou si un
  onglet, un secret ou un prompt est ajouté, supprimé, déplacé ou renommé
  entre-temps, une ligne d'onglet, de secret ou de prompt ne fait rien. Les
  commandes s'exécutent quand même.

### Rechercher et remplacer

- **Édition > Rechercher...** (Ctrl+F) ouvre la fenêtre **Rechercher** ;
  **Édition > Remplacer...** (Ctrl+H) ouvre **Rechercher et remplacer**. Le
  texte sélectionné, s'il y en a, sert de recherche de départ.
- **Rechercher :** le texte cherché. **Précédent** et **Suivant** passent d'une
  occurrence à l'autre. Le compteur affiche « 3 / 12 », « 12 occurrences » ou
  « Aucune correspondance ».
- Options : **Respecter la casse**, **Mot entier**, **Expression régulière**.
- Dans Rechercher et remplacer : **Remplacer :** le nouveau texte, **Remplacer**
  (l'occurrence courante), **Tout remplacer** (« N occurrences remplacées. »).
- **Fermer** ferme la fenêtre.

**Expression régulière.** La case n'est pas mémorisée : elle est décochée à
chaque ouverture de la fenêtre.

- `^` et `$` correspondent au début et à la fin de chaque ligne ; `\n`
  correspond à un saut de ligne.
- **Respecter la casse** s'applique toujours (décochée : casse ignorée, lettres
  accentuées comprises). **Mot entier** entoure le motif : `\b(?:motif)\b`.
- Un motif invalide, ou trop gros (plus de 1 Mio une fois compilé), affiche
  « Expression régulière invalide » et ne trouve rien. `\p{L}` et les autres
  classes de propriétés Unicode ne sont pas disponibles et comptent comme
  invalides ; `\w`, `\d`, `\s` et `\b` gèrent bien l'Unicode. Un motif vide ne
  fait rien.
- Dans le remplacement, `$1`, `${2}` et `${nom}` insèrent un groupe et `$$` un
  `$`. Écrivez `${1}x` quand une lettre suit un numéro de groupe.
- **Remplacer** ne remplace que si la sélection est exactement une
  correspondance ; sinon il passe à la suivante. **Tout remplacer** fait une
  modification annulable par correspondance.
- Les correspondances vides (`^`, `x*`) fonctionnent : Suivant et Précédent
  avancent toujours, et remplacer `^` par « > » préfixe chaque ligne.

### Rechercher dans tous les onglets

Édition > Rechercher dans tous les onglets... (Ctrl+Maj+F). Indisponible quand
LockNote est verrouillé. La fenêtre principale est inutilisable tant que cette
fenêtre est ouverte.

- **Rechercher :** est rempli avec la sélection si elle tient sur une ligne,
  et la recherche part tout de suite (une sélection sur plusieurs lignes
  ouvre un champ vide). **Respecter la casse** : sans elle, la casse est
  ignorée, mais pas les accents.
- La liste se met à jour avec Entrée dans le champ, quand la case change, ou
  0,3 s après la dernière touche. La ligne d'état affiche « N occurrences » ou
  « Aucune correspondance ».
- Ce qui est cherché, dans cet ordre :
  - tous les onglets, l'actif avec ses modifications non enregistrées : une
    ligne par occurrence, « <nom de l'onglet> : <numéro de ligne> :
    <extrait> » ;
  - les secrets du coffre dont le nom ou la catégorie correspond :
    « Coffre : <catégorie> / <nom> ». Les valeurs des secrets ne sont jamais
    cherchées ni affichées ;
  - les prompts dont le nom ou le texte correspond : « Prompt : <nom> ».
- Au plus 1000 résultats ; une dernière ligne indique alors « Résultats
  suivants non affichés (limite : 1000) ».
- **Afficher**, Entrée dans la liste ou un double-clic ouvre le résultat : un
  résultat d'onglet affiche l'onglet et sélectionne l'occurrence ; un résultat
  de coffre ouvre le panneau Coffre sur ce secret (ou met son nom dans le
  champ de recherche du panneau si le coffre a changé) ; un résultat de prompt
  ouvre le panneau Prompts sur ce prompt (sa recherche est gardée sauf si elle
  le masquerait).
- Si LockNote se verrouille, la fenêtre se ferme et rien n'est ouvert.

### Aller à la ligne

Édition > Aller à la ligne... (Ctrl+G). Le champ contient la ligne courante ;
tapez un numéro entre 1 et le nombre de lignes indiqué (« Numéro de ligne :
(1 - N): »), puis OK. Sinon : « Numéro de ligne invalide. »

### Aller au titre

Édition > Aller au titre... (Ctrl+Maj+O) liste les titres Markdown de l'onglet
actif, indentés selon leur niveau :

- `#` à `######` suivis d'une espace (jusqu'à 3 espaces devant) ; les `#` de
  fermeture sont enlevés et un titre vide n'est pas listé ;
- une ligne non vide suivie d'une ligne d'au moins 3 `=` (niveau 1) ou `-`
  (niveau 2) ;
- les lignes d'un bloc de code délimité par trois accents graves sont ignorées
  (un bloc jamais fermé va jusqu'à la fin de l'onglet).

Le champ **Filtre :** réduit la liste au fil de la frappe (sans tenir compte de
la casse, n'importe où dans le titre) ; la première correspondance est
sélectionnée. Entrée, OK ou un double-clic place le curseur au début de la
ligne de ce titre et ferme la fenêtre ; Échap ou Annuler la ferme sans rien
déplacer. Une indication s'affiche si l'onglet n'a aucun titre ou si rien ne
correspond. Si le raccourci de sauvegarde est Ctrl+Maj+O, il enregistre.

### Comparer avec le dernier enregistrement

Onglet > Comparer avec le dernier enregistrement... compare l'onglet actif tel
qu'au dernier enregistrement (ou tel que chargé au démarrage ou au
déverrouillage) avec son texte actuel, dans une fenêtre en lecture seule :

- des en-têtes de bloc `@@ -a,b +c,d @@` (ligne de début et nombre de lignes
  dans le texte enregistré, puis dans le texte actuel), puis des lignes qui
  commencent par `- ` (supprimée), `+ ` (ajoutée) ou deux espaces (inchangée),
  avec 3 lignes inchangées autour de chaque modification ;
- le texte utilise la police de l'éditeur, n'est pas renvoyé à la ligne et
  défile dans les deux sens ; Fermer, Entrée ou Échap ferme la fenêtre.

Un texte identique affiche à la place « Aucune modification depuis le dernier
enregistrement. ». Les fins de ligne seules et un saut de ligne final ne
comptent pas. Au-delà de 20000 lignes d'un côté, un message indique que
l'onglet est trop long pour être comparé. Un nouvel onglet, ou un onglet
importé d'une sauvegarde, compte comme jamais enregistré : toutes ses lignes
apparaissent comme ajoutées jusqu'au prochain enregistrement. La copie
enregistrée n'est gardée qu'en mémoire et effacée au verrouillage.

### Position du curseur

La barre d'état affiche « Ligne X, col. Y » (à partir de 1 ; lignes réelles,
comme dans la marge des numéros, donc le retour à la ligne automatique ne les
change pas ; une tabulation ou un émoji compte pour une colonne), et
« (N sélectionnés) » quand du texte est sélectionné, un saut de ligne comptant
pour 1. Pour une sélection faite à l'envers, la position est celle du curseur,
au début de la sélection. L'affichage suit en 200 ms environ. Dans une fenêtre
étroite, il est masqué s'il ne tient pas.

### Zoom

- Ctrl+molette sur l'éditeur ou les numéros de ligne, Ctrl+= ou Ctrl+Pavé+
  agrandissent ; Ctrl+- ou Ctrl+Pavé- réduisent ; Ctrl+0 revient à la taille
  réglée dans les Préférences. Les touches suivent la disposition du clavier
  (sur un clavier AZERTY, Ctrl+- est la touche 6).
- 1 pt par cran, entre 6 et 48 pt. Une taille des paramètres au-delà de 48 pt
  (jusqu'à 72) est gardée par le zoom avant, ramenée à 48 par le zoom arrière
  et rétablie par Ctrl+0.
- Les numéros de ligne, l'interligne et le bandeau de la ligne courante suivent
  le zoom. La vue, la sélection et son sens sont gardés.
- Le zoom ne vaut que pour la session : il n'est jamais enregistré, ne marque
  pas la note comme modifiée et ne crée pas d'étape d'annulation. Pas de zoom
  quand LockNote est verrouillé.
- Les combinaisons AltGr tapent toujours leurs caractères (AltGr+0 = @ et
  AltGr+= = } sur un clavier AZERTY). Ctrl+= ne passe plus en indice.

### Retour à la ligne

Affichage > Retour à la ligne. Actif par défaut. Le choix est enregistré avec la
note.

### Numéros de ligne

Affichage > Numéros de ligne affiche ou masque les numéros de ligne à gauche du
texte. Actif par défaut. Masqués, le texte prend leur largeur. Le choix est
enregistré avec la note (au prochain enregistrement, y compris celui de la
fermeture) et retrouvé au démarrage suivant.

### Toujours au premier plan

Affichage > Toujours au premier plan garde la fenêtre au-dessus des autres. Ce
choix n'est pas mémorisé : il est désactivé à chaque démarrage.

### Police

Fichier > Paramètres... > Préférences... : **Police de l'éditeur :** (Consolas,
Courier New, Lucida Console, Cascadia Mono, Segoe UI ; Consolas par défaut) et
**Taille de police :** (8 à 24 ; 11 par défaut). La nouvelle police s'applique
dès OK, sans redémarrer, et met fin au zoom. La police d'une sauvegarde
importée s'applique aussi tout de suite. Si la police ne peut pas être
appliquée, un message indique que l'éditeur garde sa police actuelle.

### Imprimer

Fichier > Imprimer... (Ctrl+P).

1. Confirmez votre mot de passe (fenêtre « Confirmer le mot de passe » :
   « Saisissez votre mot de passe pour imprimer : »).
2. Choisissez l'imprimante dans la boîte d'impression de Windows.

- Si du texte est sélectionné, seule la sélection est imprimée ; sinon tout
  l'onglet actif.
- La police et la taille de l'éditeur sont utilisées, avec des marges
  d'environ 2,5 cm (1 pouce). Les lignes longues sont coupées à une espace ou
  une tabulation (un mot plus large que la ligne est coupé) ; l'indentation est
  gardée. Les tabulations s'alignent tous les 1,27 cm (un demi-pouce), comme
  dans l'éditeur.
- Chaque page affiche le nom de l'onglet en haut et « Page N / M » en bas. Une
  note vide imprime une page.
- Une erreur (boîte d'impression, pilote d'imprimante, page trop petite pour
  la police) s'affiche dans un message « Impression ». Si LockNote se
  verrouille pendant que la fenêtre du mot de passe ou d'impression est
  ouverte, rien n'est imprimé.

### Exporter en fichier texte

Fichier > Exporter...

1. Confirmez votre mot de passe (« Saisissez votre mot de passe pour
   exporter : »).
2. **Encodage d'exportation** : choisissez **UTF-8** (par défaut),
   **UTF-8 with BOM**, **Windows-1252** ou **ISO-8859-1 (Latin-1)**, puis OK.
3. Choisissez le nom du fichier (Fichiers texte (*.txt)).

**Attention.** Seul l'onglet actif est exporté, et **le fichier n'est pas
chiffré**. Si des caractères ne peuvent pas être écrits dans l'encodage choisi,
ils sont remplacés et LockNote demande s'il faut continuer (« Avertissement
d'exportation »).

### Importer un fichier texte (glisser-déposer)

Déposez un fichier depuis l'Explorateur sur la fenêtre de LockNote. Une fenêtre
« Fichier déposé » propose :

- pour un fichier texte : **Insérer le texte** (à l'emplacement du curseur),
  **Ajouter en pièce jointe** ou **Annuler** ;
- pour un autre fichier : **Ajouter en pièce jointe** ou **Annuler**.

Limites : seul le premier fichier d'un dépôt multiple est pris ; les dossiers
sont refusés ; le texte inséré doit être en UTF-8 et faire au plus 32 Mio,
sinon « Ce fichier n'a pas pu être lu comme du texte. »

## 9. Coffre

**Ce que ça fait.** Le coffre range des secrets (mots de passe, clés d'API,
valeurs sur plusieurs lignes comme des clés ou des certificats) par catégorie,
chiffrés avec la note.

**Où c'est.** Le bouton **Coffre** de la barre d'onglets, Outils > Gestionnaire
de coffre... ou **Ctrl+B**. Le panneau est ouvert au démarrage de LockNote.

**Le panneau.**

- Un champ de recherche (« Rechercher un nom ») en haut.
- Une arborescence : chaque catégorie, triée par nom, avec ses secrets
  dessous. Seul le nom d'un secret est affiché, jamais sa valeur.
- Des boutons : **Ajout.**, **Modif.**, **Suppr.**, **Copier**, **Voir**.
  Modif., Suppr., Copier et Voir demandent un secret sélectionné.

**Copier un secret.** Cliquez sur le secret dans l'arborescence, ou
sélectionnez-le et cliquez sur **Copier**. La valeur part dans le
presse-papiers et « Copié : nom » s'affiche 2 secondes. La copie est exclue de
l'historique du presse-papiers de Windows (Win+V) et est effacée par
l'effacement automatique du presse-papiers. Cliquer sur une catégorie ne copie
rien.

**Voir.** **Voir** affiche la valeur pendant 5 secondes dans un champ en lecture
seule sous l'arborescence, puis la masque.

**Ajouter ou modifier un secret.**

1. **Ajout.** (ou sélectionnez un secret puis **Modif.**).
2. Dans **Nouveau secret** / **Modifier le secret** : **Nom :** (obligatoire),
   **Catégorie :**, **Valeur :**.
3. La valeur est masquée sur une ligne. **Afficher** passe à un champ sur
   plusieurs lignes qui montre toute la valeur ; recliquez sur le bouton
   (libellé « Hide ») pour la masquer.
4. **Copier** copie la valeur. **Générer...** ouvre le générateur de mots de
   passe ; son bouton **Utiliser** place le mot de passe dans la valeur.
5. OK enregistre le secret dans le coffre ; Annuler ferme sans rien changer.

**Supprimer un secret.** Sélectionnez-le, **Suppr.**, puis confirmez
(« Voulez-vous vraiment supprimer ce secret ? »).

**Recherche.** Tapez dans le champ de recherche : l'arborescence ne montre plus
que les secrets dont le nom contient le texte, sans tenir compte de la casse ni
des accents. La recherche ne porte jamais sur les valeurs.

**Catégories.** Clic droit dans l'arborescence :

- **Nouvelle catégorie…** (n'importe où) ;
- **Renommer…** et **Supprimer** (sur une catégorie).

Règles : 1 à 40 caractères, ni `:`, ni `[`, ni `]`, noms uniques (sans tenir
compte de la casse ni des accents), 50 catégories au plus. **General** existe
toujours et ne peut être ni renommée ni supprimée. Supprimer une catégorie
déplace ses secrets dans General après confirmation. Une nouvelle note commence
avec General, E-mail, Cloud, Finances, Clés API, Mots de passe et Crypto
quand elle est créée en français (General garde son nom dans toutes les
langues ; en anglais : Email, Finance, API Keys, Passwords). Les notes créées
avant gardent leurs catégories telles quelles, quelle que soit la langue.

**Glisser-déposer.** Faites glisser un secret sur une catégorie (ou sur un
secret de cette catégorie) pour l'y déplacer. Échap annule.

**Après une modification**, le coffre est enregistré au prochain
enregistrement de la note.

## 10. Prompts

**Ce que ça fait.** Range des textes que vous réutilisez (par exemple des
prompts pour un outil d'IA) et les copie en un clic.

**Où c'est.** Le bouton **Prompts** de la barre d'onglets, ou Outils > Prompts.

**Mode d'emploi.**

- **Rechercher (nom ou texte)** : filtre sur le nom et le texte, sans tenir
  compte de la casse ni des accents. La ligne d'état affiche le nombre
  (« N prompt(s) », « N sur M prompt(s) »).
- La liste affiche « Catégorie · Nom », triée par catégorie puis par nom.
  Sélectionner un prompt affiche son texte dans l'aperçu en dessous.
- **Un clic sur un prompt copie son texte** (« Copié : nom »). Le bouton
  **Copier** fait de même.
- **Insérer** met le prompt sélectionné dans la note : il remplace la
  sélection de l'éditeur, ou s'insère au curseur. Un Ctrl+Z l'annule ; la
  ligne d'état affiche « Inséré : nom ». Rien n'est inséré quand LockNote est
  verrouillé. La palette de commandes (Ctrl+Maj+P) a une ligne « Prompt :
  nom » par prompt qui fait de même, même panneau fermé.
- **Ajout.** / **Modif.** : **Nom :**, **Catégorie :** (texte libre, avec la
  liste des catégories déjà utilisées, « Général » en tête) et **Texte :**
  (plusieurs lignes ; Entrée ajoute une ligne, Tab une tabulation, Ctrl+Tab
  quitte le champ). Les trois sont obligatoires.
- **Suppr.** supprime le prompt sélectionné après confirmation.

**Variables.** Le texte d'un prompt peut contenir `{date}` (AAAA-MM-JJ),
`{time}` (HH:MM:SS), `{datetime}` (AAAA-MM-JJ HH:MM:SS), `{clipboard}` (le
texte actuel du presse-papiers, vide s'il n'y en a pas) et `{tab}` (le nom de
l'onglet actif). La date et l'heure sont l'heure locale de l'ordinateur, la
même horloge et le même format qu'Insérer un horodatage (F5) ; aucun des deux
n'indique de fuseau horaire. Écrivez `{{` et `}}` pour des accolades
littérales. Les noms inconnus (`{xyz}`), une autre casse (`{Date}`) et les
accolades isolées restent tels quels. Les variables sont remplacées quand le
prompt est copié ou inséré ; le prompt enregistré les garde.

**Limites.** Un texte fait au plus 1 Mio. Une catégorie ne peut pas contenir
`:`.

**Attention.** Un prompt n'est pas traité comme un secret : sa copie n'est pas
effacée par l'effacement automatique du presse-papiers et n'est pas exclue de
l'historique du presse-papiers.

## 11. Gestionnaire TOTP

**Ce que ça fait.** Affiche les codes à usage unique à 6 chiffres en cours
(RFC 6238, période de 30 secondes) des clés TOTP enregistrées dans la note.

**Où c'est.** Le bouton **TOTP** de la barre d'onglets, ou Outils >
Gestionnaire TOTP.

**Le panneau.** Une liste « Nom — code », rafraîchie chaque seconde, et le temps
restant avant le code suivant (« Prochain code dans N s »). Boutons : **Copier le
code**, **Ajouter une clé**, **Supprimer**.

**Copier un code.** Sélectionnez une entrée, puis **Copier le code** (« Code
copié dans le presse-papiers. »). Si Windows refuse la copie (une autre
application tient le presse-papiers), le panneau affiche « Échec de la
copie : … » à la place : rien n'a été copié. La même règle vaut pour tous les
boutons Copier de LockNote.

**Ajouter une clé.**

1. **Ajouter une clé**, puis tapez le **Nom de l’entrée :**.
2. Dans la fenêtre **Configuration TOTP**, le champ **Secret :** est vide
   (« Base32 ou lien otpauth:// »). Collez le secret donné par le site : le
   texte Base32 (espaces et minuscules acceptés, par exemple
   `jbsw y3dp ehpk 3pxp`) ou le lien `otpauth://totp/...` de son code QR. Ou
   cliquez sur **Générer** pour un nouveau secret aléatoire, puis saisissez-le
   sur le site ou dans votre application d'authentification (**Copier**, ou
   l'**URI OtpAuth :** affichée dessous).
3. **Confirmer** ajoute la clé. Un secret non valide est refusé avec la raison
   (vide, pas du Base32, longueur que le Base32 ne donne jamais parce qu'une
   partie manque ou qu'il a été collé deux fois, moins de 80 bits, lien autre
   que `otpauth://totp/`, lien sans secret, ou lien qui demande d'autres codes
   que SHA1 / 6 chiffres / 30 secondes) et rien n'est ajouté. **Vérifier** est facultatif ici : tapez un code et
   cliquez dessus pour contrôler le secret avant de confirmer.
4. Saisissez sur le site le code que LockNote affiche alors dans le panneau.

**Supprimer une clé.** Sélectionnez-la, **Supprimer**, puis confirmez (« Cette
action est irréversible. »).

**Attention.** Un code TOTP copié n'est pas effacé par l'effacement automatique
du presse-papiers. Les clés sont enregistrées avec la note et font partie d'une
sauvegarde complète.

## 12. Authentification à deux facteurs au déverrouillage

**Ce que ça fait.** Demande, au démarrage de LockNote et après le mot de passe,
un code à 6 chiffres donné par une application d'authentification.

**Où c'est.** Fichier > Paramètres... > Sécurité... > **Activer
l'authentification TOTP à deux facteurs**.

**L'activer.**

1. Cochez la case. La fenêtre **Configuration TOTP** s'ouvre avec un nouveau
   secret.
2. Ajoutez le secret (Base32 ou URI OtpAuth) à votre application
   d'authentification.
3. Tapez un code, **Vérifier**, puis **Confirmer**.
4. Cliquez sur OK dans la fenêtre Sécurité, puis enregistrez la note.

Fermer la fenêtre de configuration sans confirmer décoche la case.

**Au démarrage.** Après le mot de passe, la fenêtre **Authentification à deux
facteurs** demande : « Entrez le code à 6 chiffres de votre application
d'authentification. » Tapez-le et cliquez sur **Vérifier**. 3 tentatives ;
après le 2e mauvais code, il faut attendre 2 s. Après le 3e : « Nombre maximum
de tentatives atteint. » et LockNote se ferme. **Annuler** ferme LockNote.

**La désactiver.** Décochez la case : LockNote demande d'abord un code en cours.
Un mauvais code laisse l'option active.

**Attention.**

- Le code n'est demandé qu'au démarrage. Le déverrouillage de l'écran verrouillé
  ne demande que le mot de passe.
- Gardez le secret en lieu sûr : sans l'application ni le secret, vous ne
  pourrez plus ouvrir vos notes.
- Importer une sauvegarde complète ou un autre LockNote remplace ce réglage par
  celui des données importées.

## 13. Pièces jointes

**Ce que ça fait.** Range des fichiers dans la note chiffrée.

**Où c'est.** Le bouton **Fichiers** de la barre d'onglets, Outils > Pièces
jointes, ou **Ctrl+J**. Vous pouvez aussi déposer un fichier sur la fenêtre et
choisir **Ajouter en pièce jointe**.

**Le panneau.** Une liste « nom (taille) », le nombre de fichiers et leur taille
totale. Boutons : **Ajout.**, **Ouvrir**, **Enreg.**, **Suppr.**

- **Ajout.** : choisissez un fichier (« Joindre un fichier »).
- **Enreg.** : choisissez où écrire une copie de la pièce jointe sélectionnée.
- **Ouvrir** (ou double-clic) : choisissez où écrire une copie, puis LockNote
  ouvre cette copie avec son programme par défaut. Pour un type exécutable
  (`.exe`, `.bat`, `.cmd`, `.ps1`, `.js`, `.lnk`, `.msi`, `.reg` et
  semblables), LockNote demande d'abord : « Ouvrir la pièce jointe ? ».
- **Suppr.** : supprime la pièce jointe sélectionnée après confirmation.

**Limites.** 10 Mo par fichier et 50 Mo au total. Un fichier au-delà est
refusé. Avant Ouvrir, LockNote vérifie le SHA-256 des données ; des données
abîmées donnent « Pièce jointe corrompue (SHA-256 invalide). »

**Attention.** Ouvrir et Enreg. écrivent le fichier **en clair** à l'endroit
choisi. Supprimez vous-même cette copie quand vous n'en avez plus besoin.

## 14. Stéganographie

**Ce que ça fait.** Cache le texte de l'onglet actif, chiffré, dans une copie
d'une image (JPEG, PNG ou BMP). L'image a toujours le même aspect.

**Cacher.** Outils > Stéganographie > Cacher dans une image...

1. **Choisir l’image porteuse** : l'image de départ.
2. **Enregistrer l’image stéganographique** : où écrire la nouvelle image.
3. **Mot de passe de l’image** : un mot de passe pour cette image, tapé deux
   fois (« Mot de passe pour protéger cette image : », « Confirmer le mot de
   passe : »). Il peut différer du mot de passe de la note.
4. « Contenu caché avec succès. » affiche la taille.

**Extraire.** Outils > Stéganographie > Extraire depuis une image...

1. **Choisir l’image à extraire**.
2. **Mot de passe de l’image** : « Mot de passe utilisé pour cacher les
   données : ».
3. Le texte s'ouvre dans un nouvel onglet nommé d'après le fichier image. Si 20
   onglets sont déjà ouverts, il remplace le texte de l'onglet actif.

**Limites et précautions.**

- Seul le texte de l'onglet actif est caché (pas les autres onglets, ni le
  coffre, ni les pièces jointes).
- Si les données cachées dépassent 50 % de la taille de l'image : « Notes trop
  volumineuses pour cette image. » Choisissez une image plus grande.
- « Ne pas partager via WhatsApp, Instagram, etc. La recompression détruit les
  données cachées. »
- « Mot de passe incorrect ou données non LockNote. » : par conception,
  LockNote ne peut pas distinguer les deux cas. « Aucune donnée cachée dans
  cette image. » signifie qu'il n'y a rien après la fin de l'image.
- Les formats autres que JPEG, PNG et BMP sont refusés (« Format non supporté
  (JPEG, PNG, BMP). »).

## 15. Clés SSH et ssh-agent

### Générer une clé

Outils > Clés SSH > Générer une clé SSH...

1. **Type :** Ed25519 (recommandé, par défaut), ECDSA P-256, ECDSA P-384,
   RSA 3072 ou RSA 4096.
2. **Nom dans le Vault :**, **Commentaire :** (une ligne, sans tabulation ni
   caractère de contrôle), **Passphrase (facultative) :** et **Confirmer la
   passphrase :**. Avec une passphrase, la clé privée est chiffrée au format
   OpenSSH standard.
3. **Générer** (« Génération... » : RSA 4096 prend quelques secondes).
   L'**Empreinte :** (`SHA256:...`) et la **Clé publique :** s'affichent.
4. Ensuite :
   - **Copier la clé publique** ;
   - **Enregistrer (Vault)** : range deux secrets dans la catégorie
     **SSH Keys** (créée au besoin) : `<nom>` (clé privée) et `<nom>.pub` (clé
     publique). Un nom déjà utilisé est refusé ;
   - **Exporter...** : écrit `<fichier>` et `<fichier>.pub`. Le fichier privé
     n'est lisible que par votre compte Windows. LockNote n'écrase jamais un
     fichier. Sans passphrase, il demande d'abord confirmation.
5. **Fermer**. Une clé ni enregistrée ni exportée est abandonnée.

### Charger une clé dans ssh-agent

Outils > Clés SSH > Charger dans ssh-agent...

1. **Clé :** choisissez un secret du coffre qui contient une clé privée OpenSSH
   (dans n'importe quelle catégorie).
2. **Passphrase (si la clé en a une) :**.
3. Lisez l'avertissement, puis **Charger**. « Clé chargée dans ssh-agent »
   affiche son empreinte.

**Ce que fait LockNote ensuite.** La clé est confiée au service agent OpenSSH de
Windows. LockNote la retire de l'agent au verrouillage et à la fermeture et,
après un arrêt anormal, au démarrage ou au déverrouillage suivant de ce fichier.
Le chargement enregistre aussi le fichier. Une même clé ajoutée à la main avec
`ssh-add` est retirée aussi.

**Erreurs.** Si le service n'est pas démarré, LockNote affiche les commandes à
lancer dans un PowerShell administrateur :

```powershell
Get-Service ssh-agent | Set-Service -StartupType Automatic
Start-Service ssh-agent
```

Autres messages : « Passphrase incorrecte. », « Cette clé est protégée :
saisissez sa passphrase. », « ssh-agent a refusé l'opération. », « ssh-agent
n'a pas répondu à temps. ». Si aucun secret ne contient de clé privée :
« Aucune clé privée SSH dans le Vault. »

## 16. Générateur de mots de passe

**Où c'est.** Outils > Générateur de mots de passe..., et le bouton
**Générer...** de la fenêtre d'un secret du coffre.

**Mode d'emploi.**

- Un mot de passe est tiré à l'ouverture, sur **Générer**, et à chaque
  changement d'option. **Afficher** / **Masquer** le montre ou le cache.
- **Copier** le copie (exclu de l'historique du presse-papiers, effacé par
  l'effacement automatique du presse-papiers).
- **Utiliser** (seulement depuis la fenêtre d'un secret) ferme le générateur et
  place le mot de passe dans la valeur du secret.
- **Fermer** ferme la fenêtre.

**Options et valeurs par défaut.**

| Option | Par défaut |
|---|---|
| Longueur (8-64) | 16 |
| Majuscules (A-Z), Minuscules (a-z), Chiffres (0-9), Symboles | toutes cochées |
| Symboles autorisés | ``!@#$%^&*()-_=+[]{};:,.?`` (ponctuation ASCII uniquement, doublons retirés) |
| Exclure les caractères ambigus (0Oo1lI\|) | non cochée |

Le mot de passe contient toujours au moins un caractère de chaque type coché.
Quand les options le rendent impossible, le champ est vidé et la raison
s'affiche (par exemple « Sélectionnez au moins un type de caractère. »). Les
dernières options utilisées sont conservées avec la note.

## 17. Verrouillage auto, sauvegarde auto et effacement du presse-papiers

### Verrouillage auto

Fichier > Paramètres... > Sécurité... > **Verrouillage auto :** Désactivé, 1, 5,
10, 15 (par défaut), 30 ou 60 min.

LockNote se verrouille ([section 4](#4-verrouillage-et-écran-verrouillé)) quand,
pendant cette durée, aucune touche n'a été frappée, la souris n'a ni bougé ni
cliqué sur sa fenêtre et le texte n'a pas changé. Taper, cliquer ou bouger la
souris dans une fenêtre ou un message de LockNote (une boîte de dialogue comme
celle où vous tapez un secret, une question) compte aussi ; ce que vous faites
dans d'autres applications ne compte pas. Il vérifie toutes les 10 secondes.
Les modifications non enregistrées sont d'abord enregistrées ; si cet
enregistrement échoue, LockNote ne se verrouille pas et réessaie après une
nouvelle période. Une nouvelle valeur s'applique tout de suite.

Tant qu'une fenêtre ou un message de LockNote est ouvert sur la fenêtre
principale (une boîte de dialogue, la question de fermeture…), la sauvegarde
auto attend et son délai repart à sa fermeture. Elle attend aussi pendant que
les notes sont masquées (voir plus bas) : seules les tentatives
d'enregistrement de LockNote ont lieu alors.

**Quand LockNote se verrouille** (délai du verrouillage auto atteint, bouton
Verrouiller, Ctrl+L), vos notes sont masquées tout de suite (« Notes
masquées ») ; seul votre mot de passe les réaffiche (**Afficher les notes** ;
les mots de passe erronés sont ralentis comme sur l'écran de verrouillage).
À ce moment, chaque fenêtre ou question ouverte de LockNote reçoit sa
réponse sans danger : la question de fermeture est annulée, « Fermer sans
enregistrer ? » garde LockNote ouvert, une question Oui/Non reçoit la
réponse **Non**, les autres fenêtres sont fermées comme par Annuler (ce que
vous y aviez tapé est perdu, vos notes non). Avec le délai du verrouillage
auto, cela n'arrive qu'à une fenêtre dont personne ne se sert : y taper,
cliquer ou bouger la souris compte comme une activité. Le bouton Verrouiller
et Ctrl+L ferment les fenêtres même pendant que vous vous en servez. Une
réponse cliquée ensuite n'a aucun effet.

LockNote enregistre puis se verrouille pour de bon dès qu'aucune fenêtre
n'est plus ouverte. Si une fenêtre ne peut pas se fermer tout de suite (la
fenêtre des fichiers temporaires pendant la lecture d'un partage réseau),
l'écran la nomme et LockNote attend. Si l'enregistrement échoue, rien n'est
perdu : vos notes restent masquées avec leurs modifications non
enregistrées, LockNote réessaie d'enregistrer toutes les 10 secondes, et
quand vous tapez votre mot de passe elles reviennent avec leurs
modifications et la raison de l'échec. Pour abandonner les modifications,
affichez les notes, puis fermez sans enregistrer.

LockNote ne peut pas être fermé pendant que les notes sont masquées
(Quitter le signale sur cet écran). Une fermeture de session ou un arrêt de
Windows suit sa règle habituelle ([section 6](#6-enregistrer-et-fermer)) :
les notes masquées sont enregistrées avec leurs modifications, sauf si À la
fermeture vaut **Ne jamais enregistrer** (rien n'est alors enregistré et ces
modifications sont perdues, puisque vous avez choisi de ne jamais enregistrer
à la fermeture). Si cet enregistrement échoue, Windows affiche
l'avertissement de LockNote ; un arrêt forcé malgré tout perd les
modifications non enregistrées.

Fermer LockNote (Quitter de la zone de notification) pendant qu'une fenêtre
est ouverte annule d'abord cette fenêtre, puis ferme comme d'habitude ; si
elle n'est toujours pas fermée au bout de 10 secondes, LockNote la nomme et
propose **Continuer d'attendre** ou **Fermer sans enregistrer**.

Pendant le verrouillage, les menus n'offrent que Fichier > Quitter et Aide >
À propos.

### Sauvegarde auto

Fichier > Paramètres... > Préférences... > **Sauvegarde auto :** Désactivé, 10,
30 (par défaut), 60, 120 ou 300 s.

Quand des modifications ne sont pas enregistrées, LockNote les enregistre à cet
intervalle. Il n'enregistre jamais pendant le verrouillage. Une nouvelle valeur
s'applique à partir du démarrage suivant de LockNote.

### Effacement du presse-papiers

Fichier > Paramètres... > Sécurité... > **Effacement presse-papiers :**
Désactivé, 10, 30 (par défaut), 60 ou 120 s.

Passé ce délai, LockNote vide le presse-papiers, seulement s'il contient
toujours la valeur qu'il y a copiée (vérification toutes les 5 secondes). Cela
concerne les copies d'un secret du coffre (arborescence, bouton **Copier**,
fenêtre du secret) et du générateur de mots de passe. Cela ne concerne pas le
texte copié depuis l'éditeur, les codes TOTP, les prompts ni le chemin du
fichier. Une nouvelle valeur s'applique à partir du démarrage suivant de
LockNote.

## 18. Raccourci global et zone de notification

**Raccourci global.** **Ctrl+Maj+L**, depuis n'importe quel programme, masque la
fenêtre de LockNote ou la réaffiche. Si LockNote est verrouillé, il affiche
l'écran verrouillé. Fichier > Paramètres... > Préférences... affiche ce
raccourci (**Raccourci global :**) ; il ne s'y change pas. Si un autre
programme utilise déjà cette combinaison, le raccourci ne fait rien.

**Zone de notification.** Fichier > Paramètres... > Préférences... > **Réduire
dans la zone de notification** (désactivé par défaut). Quand l'option est
active, réduire la fenêtre la masque ; cliquez sur l'icône de LockNote dans la
zone de notification pour la faire revenir. Un clic droit sur l'icône propose
**Restaurer** et **Quitter**.

## 19. Langue et thème

**Langue.** Fichier > Paramètres... > Préférences... > **Langue :** English,
Français, Español ou Deutsch (par défaut, celle de Windows à la création de la
note, English si Windows est dans une autre langue). Après OK : « Veuillez
redémarrer LockNote pour appliquer le changement de langue. » Le choix est
enregistré avec la note. Les fenêtres affichées avant le déchiffrement suivent
la langue de Windows ([section 1](#1-premier-lancement-et-mot-de-passe)).
Changer de langue ne renomme pas les onglets ni les catégories qui existent
déjà.

**Thème.** Fichier > Paramètres... > Préférences... > **Thème :** Sombre (par
défaut) ou Clair. Il s'applique tout de suite, sans redémarrage.

## 20. Sauvegardes de sécurité

**Ce que ça fait.** Avant une action qui remplace vos notes, LockNote écrit sans
rien demander une sauvegarde chiffrée de toute la note :

- avant **Importer une sauvegarde complète** et **Importer depuis un autre
  LockNote** (« Avant import ») ;
- avant de télécharger une **mise à jour** (« Avant mise à jour ») ;
- quand vous en demandez une (« Manuelle »).

Si cette sauvegarde ne peut pas être écrite, l'action n'a pas lieu (« La
sauvegarde de sécurité n'a pas pu être écrite, rien n'a donc été importé. » /
« ... la mise à jour n'a donc pas été téléchargée. »).

**Où elles sont.** `%LOCALAPPDATA%\LockNote\auto-backups\`, en fichiers
`.lnbak` chiffrés avec le mot de passe de la note au moment où elles sont
faites.

**La fenêtre.** Fichier > Sauvegardes de sécurité...

- La liste, la plus récente en premier (date en UTC, type, taille), et le détail
  de la sauvegarde sélectionnée.
- **Restaurer** : demande le mot de passe qu'avait la note à ce moment-là,
  montre ce que contient la sauvegarde, écrit une nouvelle sauvegarde « Avant
  import » de la note actuelle, puis remplace toute la note. Enregistrez pour
  garder le résultat.
- **Créer maintenant** : écrit une sauvegarde manuelle (« Sauvegarde
  créée. »).
- **Supprimer** : supprime la sauvegarde sélectionnée après confirmation.
- **Ouvrir le dossier** : ouvre le dossier dans l'Explorateur.
- **Fermer**.

**Conservation.** Après chaque nouvelle sauvegarde, LockNote supprime les
anciennes sauvegardes automatiques : il garde toujours les 3 plus récentes de
chaque type (« Avant import », « Avant mise à jour »), puis supprime celles de
plus de 30 jours, puis les plus anciennes jusqu'à ce que le dossier pèse au plus
1 Gio. Les sauvegardes manuelles ne sont jamais supprimées automatiquement.

**Après un changement de mot de passe.** Les sauvegardes gardent le mot de
passe avec lequel elles ont été faites. LockNote propose de les supprimer
(« ... restent ouvrables avec lui. Les supprimer maintenant ? »). Répondez Non
pour les garder.

**Limite.** Une restauration remplace toute la note ; on ne peut pas restaurer
un seul onglet.

## 21. Sauvegarde complète (.lnbak)

**Ce que ça fait.** Écrit tout ce que contient la note dans un seul fichier
`.lnbak` chiffré, que toute version ultérieure de LockNote sait lire.

| Inclus | Non inclus |
|---|---|
| Onglets (nom, texte, couleur), prompts, coffre (avec vos catégories), pièces jointes, paramètres (dont les clés TOTP et l'authentification à deux facteurs) | Position et taille de la fenêtre, largeur des panneaux, onglet actif et ligne du curseur, clés chargées dans ssh-agent |

### Exporter

Fichier > Exporter une sauvegarde complète...

1. Confirmez votre mot de passe.
2. Choisissez le fichier. Un nom sans extension reçoit `.lnbak` ; un fichier
   existant demande confirmation avant d'être remplacé (« Ce fichier existe
   déjà. Le remplacer ? »).
3. LockNote écrit le fichier, le relit pour le vérifier, puis affiche
   « Sauvegarde complète enregistrée dans : ... ».

Le fichier est chiffré avec le mot de passe de la note.

### Importer

Fichier > Importer une sauvegarde complète...

1. Choisissez le fichier `.lnbak`.
2. **Mot de passe de cette sauvegarde :** celui de la note dont vient la
   sauvegarde. Un mauvais mot de passe permet de réessayer.
3. La confirmation donne la date et le nombre d'onglets, de prompts, de secrets
   et de pièces jointes : « Remplacer TOUT le contenu actuel par cette
   sauvegarde ? »
4. Une sauvegarde de sécurité de la note actuelle est écrite, puis le contenu
   est remplacé dans la fenêtre.
5. « Sauvegarde importée. Enregistrez pour la conserver. » Rien n'est écrit
   tant que vous n'enregistrez pas.

**Attention.**

- Tout est remplacé, paramètres et authentification à deux facteurs compris.
- Le thème et la police de l'éditeur importés s'appliquent tout de suite ; la
  langue après enregistrement et redémarrage.
- Une sauvegarde faite par une version plus récente de LockNote est refusée
  avant tout mot de passe : mettez LockNote à jour. Les fichiers de plus de
  512 Mio sont refusés.

## 22. Importer depuis un autre LockNote

**Ce que ça fait.** Transfère les données d'un autre `.exe` LockNote dans celui
que vous utilisez, par exemple pour passer à une nouvelle copie du programme.

**Mode d'emploi.** Fichier > Importer depuis un autre LockNote...

1. Choisissez l'autre `.exe` (pas celui qui tourne : « C'est le fichier que vous
   utilisez. »). Choisissez-le par son chemin habituel, avec la lettre de
   lecteur que vous utilisez toujours : un enregistrement en attente de ce
   fichier est lié à son chemin, et LockNote lit cet enregistrement quand c'est
   le plus récent.
2. **Mot de passe du fichier LockNote que vous importez :**.
3. Confirmez le remplacement (nombre d'onglets, de prompts, de secrets, de
   pièces jointes).
4. Une sauvegarde de sécurité de la note actuelle est écrite, puis les données
   sont remplacées.
5. Enregistrez. Les données sont alors protégées par le mot de passe de **ce**
   fichier.

**Ce qui n'est pas repris.** Position de la fenêtre, onglet actif, ligne du
curseur, clés chargées dans ssh-agent. L'autre fichier est seulement lu,
jamais modifié.

## 23. Fichiers temporaires

**Où c'est.** Fichier > Paramètres... > Fichiers temporaires...

**Ce que la fenêtre montre.**

- Une explication des fichiers d'attente ([section 2](#2-où-sont-stockées-vos-notes))
  et le chemin du dossier, avec **Ouvrir le dossier** et **Copier le chemin**.
- **Sauvegardes en attente (.tmp) :** chaque `.tmp` en attente, avec sa date
  (UTC), sa taille et son état. Sélectionnez-en un pour voir le détail :
  - **Ce fichier LockNote** : l'enregistrement en attente du fichier ouvert. Il
    y est écrit à la fermeture de LockNote.
  - **Un autre fichier LockNote** : un enregistrement qui attend un autre
    fichier LockNote. Conservé.
  - **Orphelin : son fichier LockNote n'existe plus** : le fichier LockNote
    n'existe plus, mais son dossier existe.
  - **Son dossier est absent (lecteur déconnecté ?)**, **Son lecteur désigne
    maintenant un autre disque**, **Son fichier LockNote ne peut pas être
    vérifié**, **Laissé par une ancienne version**, **Fichier compagnon
    inutilisable**, **Ne peut pas être examiné** : conservés ; le cas échéant,
    rebranchez le lecteur et ouvrez ce fichier.
- **Ce fichier LockNote :** indique si un enregistrement attend le fichier
  ouvert. Si oui, **Fermer LockNote maintenant** ferme LockNote (avec la
  question d'enregistrement habituelle) pour qu'il soit écrit dans le fichier.
- **Journal des recopies** et **Journal des imports** ouvrent les fichiers
  journaux.

**Supprimer l'orphelin...** n'est actif que pour un orphelin. Après
confirmation, il supprime ce `.tmp` et son compagnon. **Les notes qu'il contient
sont perdues.** Cette fenêtre ne supprime jamais rien d'autre dans le dossier.

## 24. Mises à jour

**Où c'est.** Aide > Vérifier les mises à jour... LockNote ne vérifie que quand
vous le demandez.

**Fonctionnement.**

1. LockNote lit la dernière version publiée du dépôt public
   (github.com/ctardy/LockNote). S'il n'y en a pas de plus récente : « Vous
   utilisez la dernière version (x.y.z). »
2. Sinon : « Une nouvelle version (x.y.z) est disponible. Voulez-vous la
   télécharger ? » Répondez Oui.
3. Une sauvegarde de sécurité (« Avant mise à jour ») est écrite. Si elle
   échoue, rien n'est téléchargé.
4. LockNote télécharge `LockNote-windows-x64.zip` et le vérifie : la liste des
   empreintes `SHA256SUMS.txt` doit porter une signature Ed25519 valide, et
   l'archive doit correspondre à son empreinte. Sinon la mise à jour s'arrête
   sur une erreur.
5. Le nouveau programme est placé dans le fichier d'attente avec vos notes.
   « Redémarrer LockNote maintenant pour l'appliquer ? »
   - **Oui** : LockNote se ferme (avec un dernier enregistrement), le fichier
     programme est remplacé, et LockNote redémarre.
   - **Non** : le fichier programme est remplacé à la prochaine fermeture de
     LockNote.

Vos notes sont conservées. Si le remplacement échoue à cette fermeture, un
message indique que LockNote n'a pas été relancé et nomme le fichier journal.

## 25. Migrer depuis LockNote 1.2.x

LockNote 2.0 lit les fichiers de la 1.2.x (exe et `.lnbak`).

1. **Mettez à jour toutes vos copies de LockNote.** Un LockNote 1.2.1
   supprime, à son démarrage, les fichiers `.tmp` en attente des autres copies
   (de plus d'une minute) dans `%LOCALAPPDATA%\LockNote`, qui peuvent contenir
   leur dernier enregistrement. Ne gardez pas de copies 1.2.1 à côté de copies
   2.0.
2. **Si Aide > Vérifier les mises à jour de votre copie propose la 2.0**,
   acceptez : vos notes sont conservées. Avant cela, vérifiez qu'aucune ligne
   de votre note n'est exactement un en-tête de bloc LockNote
   (`[LOCKNOTE_VAULT]`, `[LOCKNOTE_PROMPTS]`, `[LOCKNOTE_ATTACHMENTS]`,
   `[LOCKNOTE_HISTORY]`, `[LOCKNOTE_STATE]`, ou `[LOCKNOTE_TABS]` en première
   ligne) : la 2.0 lit une telle ligne comme le début de ce bloc, et la suite de
   la note peut être perdue au prochain enregistrement. Modifiez une telle
   ligne (ajoutez un caractère).
3. **Si votre copie ne propose pas la 2.0**, migrez à la main :
   1. Dans l'ancien LockNote : Fichier > Exporter une sauvegarde complète...
      (`.lnbak`), puis fermez-le.
   2. Placez le nouveau `LockNote.exe` à côté de l'ancien exe, ouvrez-le et
      choisissez un mot de passe.
   3. Fichier > Importer une sauvegarde complète... avec le `.lnbak` de
      l'étape 1. Ou Fichier > Importer depuis un autre LockNote..., en
      choisissant l'ancien exe par son chemin habituel.
   4. Vérifiez les onglets, le coffre, les codes TOTP, les prompts et les
      pièces jointes, puis enregistrez.
   5. Avant de supprimer l'ancien exe, vérifiez dans Fichier > Paramètres... >
      Fichiers temporaires... qu'il ne lui reste aucun `.tmp` en attente.

**Non importés :** l'indice de mot de passe des anciennes versions, l'état de
session (onglet actif et ligne du curseur) et les clés chargées dans
ssh-agent.

**Ancien fichier de licence.** `%LOCALAPPDATA%\LockNote\license.dat`, laissé par
d'anciennes versions, ne sert plus. Vous pouvez le supprimer.

## 26. Référence des paramètres

Les paramètres sont stockés dans la note chiffrée et font partie d'une
sauvegarde complète (sauf les tailles de la fenêtre et des panneaux). Cliquez
sur OK pour les appliquer, puis enregistrez la note.

### Fichier > Paramètres... > Préférences...

| Section | Option | Valeurs | Par défaut | S'applique |
|---|---|---|---|---|
| Apparence | Thème : | Sombre, Clair | Sombre | tout de suite |
| Apparence | Langue : | English, Français, Español, Deutsch | langue de Windows à la création (English sinon) | après redémarrage |
| Apparence | Police de l'éditeur : | Consolas, Courier New, Lucida Console, Cascadia Mono, Segoe UI | Consolas | tout de suite |
| Apparence | Taille de police : | 8, 9, 10, 11, 12, 14, 16, 18, 20, 24 | 11 | tout de suite |
| Comportement | À la fermeture : | Demander, Toujours enregistrer, Ne jamais enregistrer | Demander | tout de suite |
| Comportement | Raccourci de sauvegarde : | modificateur(s) + A-Z ou F1-F24 | Ctrl+S | tout de suite |
| Comportement | Sauvegarde auto : | Désactivé, 10, 30, 60, 120, 300 s | 30 s | au démarrage suivant |
| Comportement | Réduire dans la zone de notification | oui / non | non | tout de suite |
| Comportement | Raccourci global : | affiché seulement | Ctrl+Shift+L | - |

### Fichier > Paramètres... > Sécurité...

| Option | Valeurs | Par défaut | S'applique |
|---|---|---|---|
| Changer le mot de passe... | ouvre la fenêtre de création de mot de passe (nouveau mot de passe deux fois) | - | après enregistrement |
| Long. min. mot de passe : | 1, 4, 6, 8, 12, 16 | 6 (4 pour les notes faites avec des versions qui n'avaient pas ce réglage) | à la prochaine création ou au prochain changement de mot de passe |
| Verrouillage auto : | Désactivé, 1, 5, 10, 15, 30, 60 min | 15 min | tout de suite |
| Effacement presse-papiers : | Désactivé, 10, 30, 60, 120 s | 30 s | au démarrage suivant |
| Activer l'authentification TOTP à deux facteurs | oui / non | non | au démarrage suivant (après enregistrement) |

**Changer le mot de passe...** : « Password changed ✓ » s'affiche dans la
fenêtre. Le nouveau mot de passe protège la note une fois OK cliqué et la note
enregistrée. LockNote propose ensuite de supprimer les sauvegardes de sécurité
faites avec l'ancien mot de passe.

### Fichier > Paramètres... > Fichiers temporaires...

Voir la [section 23](#23-fichiers-temporaires).

## 27. Raccourcis clavier

| Raccourci | Action |
|---|---|
| Ctrl+S | Enregistrer (modifiable) |
| Ctrl+Q, Alt+F4 | Quitter |
| Ctrl+F | Rechercher |
| Ctrl+H | Rechercher et remplacer |
| Ctrl+Maj+F | Rechercher dans tous les onglets |
| Ctrl+G | Aller à la ligne |
| Ctrl+Maj+O | Aller au titre |
| Ctrl+Maj+P | Palette de commandes |
| Ctrl+D | Dupliquer la ligne |
| Ctrl+Maj+K | Supprimer la ligne |
| Alt+Haut, Alt+Bas | Déplacer la ou les lignes vers le haut, vers le bas |
| Ctrl+Maj+U, Ctrl+U | MAJUSCULES, minuscules |
| Tab, Maj+Tab (sur une sélection de plusieurs lignes) | Indenter, désindenter les lignes |
| Maj+Tab | Désindenter la ligne courante |
| Ctrl+. | Ajouter ou enlever une puce sur la ligne |
| Ctrl+Entrée | Cocher ou décocher la case de la ligne, ou en ajouter une |
| Ctrl+molette, Ctrl+=, Ctrl+- | Zoom avant, zoom arrière |
| Ctrl+0 | Taille de police des Préférences |
| Ctrl+A | Tout sélectionner |
| Ctrl+Z, Ctrl+Y | Annuler, rétablir |
| Ctrl+X, Ctrl+C, Ctrl+V | Couper, copier, coller |
| Ctrl+Maj+V | Coller en texte brut |
| F5 | Insérer un horodatage |
| Ctrl+T | Nouvel onglet |
| Ctrl+W | Fermer l'onglet |
| F2 | Renommer l'onglet |
| Ctrl+Tab, Ctrl+Maj+Tab | Onglet suivant, onglet précédent |
| Ctrl+B | Panneau Coffre |
| Ctrl+J | Panneau Pièces jointes |
| Ctrl+L | Verrouiller |
| Ctrl+P | Imprimer |
| Échap | Revenir à l'éditeur |
| Ctrl+Maj+L | Afficher ou masquer LockNote, depuis n'importe quel programme |

Dans les fenêtres de dialogue, Entrée valide et Échap annule.

## 28. Ce qui est écrit en clair sur le disque

LockNote n'écrit jamais vos notes en clair, sauf quand vous le demandez :

- Fichier > Exporter... (fichier texte de l'onglet actif) ;
- **Ouvrir** et **Enreg.** d'une pièce jointe (une copie du fichier) ;
- **Exporter...** d'une clé SSH (le fichier de clé privée est protégé par sa
  passphrase si vous en avez donné une, et lisible par votre seul compte).

Les fichiers d'attente, les sauvegardes de sécurité, les sauvegardes complètes
et les images produites par la stéganographie sont chiffrés.
