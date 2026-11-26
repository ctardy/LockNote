# Guide utilisateur de LockNote 2.0.0

[English](../en/user-guide.md) | [README](../../README.fr.md)

LockNote est un bloc-notes chiffré pour Windows qui réunit le programme et vos
notes dans un seul fichier `.exe`. Rien à installer, aucun compte, rien
n'est envoyé dans un cloud. Depuis la 2.0.0, toutes les fonctions sont
gratuites ; il n'y a plus d'édition Pro.

Ce guide décrit toutes les fonctions de LockNote 2.0.0. Les noms de menus et
de boutons sont ceux de l'interface en français (Fichier > Paramètres... >
Préférences... > Langue : Français). Quelques fenêtres et messages restent en
anglais quelle que soit la langue choisie : ils sont cités en anglais, avec
leur sens en français.

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
2. La fenêtre **Create Password** (« Créer un mot de passe ») s'ouvre, en
   anglais : « Welcome to LockNote ». Tapez votre mot de passe dans
   **Password (min. 6 characters):**, puis le même dans **Confirm:**.
3. Cliquez sur **Create** (ou appuyez sur Entrée).

**Options et valeurs par défaut.**

- Longueur minimale : 6 caractères pour une nouvelle note. Elle se change
  ensuite dans Fichier > Paramètres... > Sécurité... (voir la
  [section 26](#26-référence-des-paramètres)).
- La jauge de force sous le mot de passe est indicative. Elle compte un point
  pour chacun de ces critères : au moins 8 caractères, au moins 12, au moins
  16, une minuscule, une majuscule, un chiffre, un autre caractère. 0 à 1 point
  : **Weak** (faible), 2 à 3 : **Fair** (moyen), 4 à 5 : **Strong** (fort),
  6 à 7 : **Very strong** (très fort). Seule la longueur minimale est imposée.

**Attention.**

- Les erreurs s'affichent dans la fenêtre : « Password is too short. » (trop
  court) ou « Passwords do not match. » (les deux saisies diffèrent).
- **Cancel** ferme LockNote sans rien créer.
- Un mot de passe oublié ne se récupère pas. Vos notes sont chiffrées avec lui
  (AES-256-CBC + HMAC-SHA256, clé dérivée par Argon2id, 64 Mio,
  3 itérations, 4 voies).
- Cette fenêtre, celle du déverrouillage au démarrage et les messages de
  démarrage sont en anglais : la langue choisie est enregistrée dans les notes
  chiffrées et n'est connue qu'une fois celles-ci déchiffrées.

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
pose la question avant tout mot de passe, en anglais : « LockNote - Two
versions of your notes ». Le message donne les deux dates (UTC) et le dossier
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

1. Lancez LockNote. La fenêtre **Unlock** (en anglais) demande : « Enter your
   password to unlock your notes. »
2. Tapez le mot de passe. Le bouton en forme d'œil affiche ou masque la saisie.
   « Caps Lock is on » s'affiche quand la touche Verr. Maj est active et que le
   champ a le focus.
3. Appuyez sur Entrée ou cliquez sur **Unlock**. « Decrypting... » s'affiche
   pendant le calcul de la clé.
4. Si l'authentification à deux facteurs est active, le code à 6 chiffres est
   demandé ensuite ([section 12](#12-authentification-à-deux-facteurs-au-déverrouillage)).

**Limites.**

- 5 tentatives. Après un mauvais mot de passe : « Incorrect password. (N/5) ».
  À partir de la 2e erreur, il faut attendre avant la suivante : 1 s après la
  2e, 2 s après la 3e, 4 s après la 4e (« Retry in Ns... »). Après la 5e :
  « Maximum attempts reached. Exiting. » et LockNote se ferme.
- **Cancel** ou Échap ferme LockNote.

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
| Édition | Couper, Copier, Coller, Coller en texte brut, Rechercher..., Remplacer..., Aller à la ligne..., Dupliquer la ligne, Supprimer la ligne, Tout sélectionner, Insérer un horodatage |
| Affichage | Toujours au premier plan, Retour à la ligne |
| Onglet | Nouvel onglet, Fermer l'onglet, Renommer l'onglet..., Onglet suivant, Onglet précédent |
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
- À droite : un point avec **Enregistré** ou **Modifié**, puis **Chiffré**,
  puis l'encodage (UTF-8).

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
partir du démarrage suivant.

**À la fermeture.** Fichier > Quitter, Ctrl+Q, Alt+F4 ou le bouton de fermeture
de la fenêtre. Ce qui arrive aux modifications non enregistrées dépend de
Fichier > Paramètres... > Préférences... > **À la fermeture :**

- **Demander** (par défaut) : « You have unsaved changes. Save before
  closing? » (en anglais : enregistrer avant de fermer ?). Oui enregistre, Non
  ferme sans enregistrer, Annuler laisse LockNote ouvert.
- **Toujours enregistrer** : enregistre sans demander.
- **Ne jamais enregistrer** : ferme sans enregistrer.

Sauf si vous avez choisi de ne pas enregistrer, LockNote enregistre une
dernière fois à la fermeture, ce qui conserve aussi la position de la fenêtre
et les paramètres.

**Sauvegarde auto.** Voir la [section 17](#17-verrouillage-auto-sauvegarde-auto-et-effacement-du-presse-papiers).

**Attention.** Si un enregistrement échoue, un message « Save Error » en donne
la raison. Vos modifications sont toujours dans la fenêtre : réessayez avant de
fermer.

## 7. Onglets

**Ce que ça fait.** Un fichier LockNote contient jusqu'à 20 onglets, chacun avec
son texte, son nom et une couleur facultative.

**Mode d'emploi.**

- **Nouvel onglet** : le bouton **+**, Onglet > Nouvel onglet ou **Ctrl+T**.
  Les nouveaux onglets s'appellent « Note N ». À 20 onglets : « Nombre maximum
  d’onglets atteint (20). »
- **Changer d'onglet** : cliquez sur l'onglet, ou **Ctrl+Tab** /
  **Ctrl+Maj+Tab** (Onglet > Onglet suivant / Onglet précédent).
- **Fermer** : la croix de l'onglet, Onglet > Fermer l'onglet, **Ctrl+W**, ou
  clic droit > Fermer l’onglet. Un onglet qui contient du texte demande
  d'abord : « Fermer l’onglet « nom » ? Son contenu sera perdu. » Le dernier
  onglet ne peut pas être fermé.
- **Renommer** : Onglet > Renommer l'onglet... (**F2**), ou clic droit sur l'onglet >
  Renommer l’onglet.... Le nom ne peut pas être vide.
- **Couleur** : clic droit sur l'onglet > Couleur > une couleur (affichées en
  anglais : blue, green, red, orange, purple, yellow, pink, gray), ou Aucune
  couleur. L'onglet affiche une pastille de cette couleur.
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
| Insérer un horodatage | menu Édition | F5 |
| Puce sur la ligne courante | clavier seulement | Ctrl+. (point) |

- **Insérer un horodatage** insère la date et l'heure au format
  `AAAA-MM-JJ HH:MM:SS`, en UTC.
- **Puces** : Ctrl+. ajoute « • » en début de ligne, ou l'enlève. Sur une ligne
  à puce, Entrée commence une nouvelle ligne à puce ; Entrée sur une ligne à
  puce vide termine la liste.
- **La touche Tab** insère une tabulation dans le texte.
- **Double-clic** : sélectionne tout le texte compris entre deux espaces ; un
  mot de passe comme `Pass@word1!` est donc sélectionné en entier. Rien n'est
  copié : la copie se fait toujours par Ctrl+C.
- **Surlignage des occurrences** : quand vous sélectionnez un mot (2 à 256
  caractères, sur une ligne), ses autres occurrences dans l'onglet sont
  surlignées.
- **Liens** : un texte qui commence par `http://` ou `https://` s'affiche comme
  un lien ; un clic l'ouvre dans le navigateur.
- **Numéros de ligne** : affichés dans une marge à gauche du texte.

### Rechercher et remplacer

- **Édition > Rechercher...** (Ctrl+F) ouvre la fenêtre **Rechercher** ;
  **Édition > Remplacer...** (Ctrl+H) ouvre **Rechercher et remplacer**. Le
  texte sélectionné, s'il y en a, sert de recherche de départ.
- **Rechercher :** le texte cherché. **Précédent** et **Suivant** passent d'une
  occurrence à l'autre. Le compteur affiche « 3 / 12 », « 12 occurrences » ou
  « Aucune correspondance ».
- Options : **Respecter la casse**, **Mot entier**.
- Dans Rechercher et remplacer : **Remplacer :** le nouveau texte, **Remplacer**
  (l'occurrence courante), **Tout remplacer** (« N occurrences remplacées. »).
- **Fermer** ferme la fenêtre.

### Aller à la ligne

Édition > Aller à la ligne... (Ctrl+G). Le champ contient la ligne courante ;
tapez un numéro entre 1 et le nombre de lignes indiqué (« Numéro de ligne :
(1 - N): »), puis OK. Sinon : « Numéro de ligne invalide. »

### Retour à la ligne

Affichage > Retour à la ligne. Actif par défaut. Le choix est enregistré avec la
note.

### Toujours au premier plan

Affichage > Toujours au premier plan garde la fenêtre au-dessus des autres. Ce
choix n'est pas mémorisé : il est désactivé à chaque démarrage.

### Police

Fichier > Paramètres... > Préférences... : **Police de l'éditeur :** (Consolas,
Courier New, Lucida Console, Cascadia Mono, Segoe UI ; Consolas par défaut) et
**Taille de police :** (8 à 24 ; 11 par défaut). L'éditeur utilise la nouvelle
police à partir du démarrage suivant de LockNote.

### Imprimer

Fichier > Imprimer... (Ctrl+P).

1. Confirmez votre mot de passe (fenêtre en anglais « Confirm Password » :
   « Enter your password to print: »).
2. Choisissez l'imprimante dans la boîte d'impression de Windows.

Seul l'onglet actif est imprimé, en Consolas 10 pt avec des marges de 2,5 cm
(1 pouce). Les lignes longues ne sont pas renvoyées à la ligne sur papier.

### Exporter en fichier texte

Fichier > Exporter...

1. Confirmez votre mot de passe (« Enter your password to export: »).
2. **Encodage d'exportation** : choisissez **UTF-8** (par défaut),
   **UTF-8 with BOM**, **Windows-1252** ou **ISO-8859-1 (Latin-1)**, puis OK.
3. Choisissez le nom du fichier (Text Files (*.txt)).

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

**Supprimer un secret.** Sélectionnez-le, **Suppr.**, puis confirmez (message
en anglais « Are you sure you want to delete this secret? »).

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
avec General, Email, Cloud, Finance, API Keys, Passwords et Crypto (noms en
anglais).

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
- **Ajout.** / **Modif.** : **Nom :**, **Catégorie :** (texte libre, avec la
  liste des catégories déjà utilisées, « Général » en tête) et **Texte :**
  (plusieurs lignes ; Entrée ajoute une ligne, Tab une tabulation, Ctrl+Tab
  quitte le champ). Les trois sont obligatoires.
- **Suppr.** supprime le prompt sélectionné après confirmation.

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
restant avant le code suivant (« Next code in N s »). Boutons : **Copier le
code**, **Ajouter une clé**, **Supprimer**.

**Copier un code.** Sélectionnez une entrée, puis **Copier le code** (« Code
copié dans le presse-papiers. »).

**Ajouter une clé.**

1. **Ajouter une clé**, puis tapez le **Nom de l’entrée :**.
2. La fenêtre **Configuration TOTP** crée un nouveau secret aléatoire et
   l'affiche (**Secret Base32 :**, avec **Copier**) ainsi que sous forme
   d'**URI OtpAuth :**.
3. Saisissez ce secret dans votre application d'authentification.
4. Tapez un code donné par l'application, cliquez sur **Vérifier**
   (« Vérifié ! »), puis sur **Confirmer**. Confirmer reste grisé tant qu'aucun
   code n'a été vérifié.

La fenêtre de configuration n'a pas de champ pour saisir un secret fourni par
un autre service : le secret est toujours créé par LockNote.

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

Fichier > Paramètres... > Sécurité... > **Verrouillage auto :** Off, 1, 5, 10,
15 (par défaut), 30 ou 60 min.

LockNote se verrouille ([section 4](#4-verrouillage-et-écran-verrouillé)) quand,
pendant cette durée, aucune touche n'a été frappée, la souris n'a ni bougé ni
cliqué sur sa fenêtre et le texte n'a pas changé. Il vérifie toutes les
10 secondes. Les modifications non enregistrées sont d'abord enregistrées ; si
cet enregistrement échoue, LockNote ne se verrouille pas et réessaie après une
nouvelle période. Une nouvelle valeur s'applique tout de suite.

### Sauvegarde auto

Fichier > Paramètres... > Préférences... > **Sauvegarde auto :** Off, 10,
30 (par défaut), 60, 120 ou 300 sec.

Quand des modifications ne sont pas enregistrées, LockNote les enregistre à cet
intervalle. Il n'enregistre jamais pendant le verrouillage. Une nouvelle valeur
s'applique à partir du démarrage suivant de LockNote.

### Effacement du presse-papiers

Fichier > Paramètres... > Sécurité... > **Effacement presse-papiers :** Off, 10,
30 (par défaut), 60 ou 120 sec.

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
Français, Español ou Deutsch (English par défaut). Après OK : « Veuillez
redémarrer LockNote pour appliquer le changement de langue. » Le choix est
enregistré avec la note. Les fenêtres de démarrage restent en anglais
([section 1](#1-premier-lancement-et-mot-de-passe)).

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
- Le thème importé s'applique tout de suite ; la langue après enregistrement et
  redémarrage.
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

**Fonctionnement.** Les messages de cette procédure sont en anglais.

1. LockNote lit la dernière version publiée du dépôt public
   (github.com/ctardy/LockNote). S'il n'y en a pas de plus récente : « You are
   running the latest version (x.y.z). » (vous êtes à jour).
2. Sinon : « A new version (x.y.z) is available. Would you like to download
   it? » Répondez Oui.
3. Une sauvegarde de sécurité (« Avant mise à jour ») est écrite. Si elle
   échoue, rien n'est téléchargé.
4. LockNote télécharge `LockNote-windows-x64.zip` et le vérifie : la liste des
   empreintes `SHA256SUMS.txt` doit porter une signature Ed25519 valide, et
   l'archive doit correspondre à son empreinte. Sinon la mise à jour s'arrête
   sur une erreur.
5. Le nouveau programme est placé dans le fichier d'attente avec vos notes.
   « Restart LockNote now to apply it? » (redémarrer maintenant ?)
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
sur OK pour les appliquer, puis enregistrez la note. Certaines listes affichent
leurs valeurs en anglais (« Off », « 10 sec », « 5 min »).

### Fichier > Paramètres... > Préférences...

| Section | Option | Valeurs | Par défaut | S'applique |
|---|---|---|---|---|
| Apparence | Thème : | Sombre, Clair | Sombre | tout de suite |
| Apparence | Langue : | English, Français, Español, Deutsch | English | après redémarrage |
| Apparence | Police de l'éditeur : | Consolas, Courier New, Lucida Console, Cascadia Mono, Segoe UI | Consolas | au démarrage suivant |
| Apparence | Taille de police : | 8, 9, 10, 11, 12, 14, 16, 18, 20, 24 | 11 | au démarrage suivant |
| Comportement | À la fermeture : | Demander, Toujours enregistrer, Ne jamais enregistrer | Demander | tout de suite |
| Comportement | Raccourci de sauvegarde : | modificateur(s) + A-Z ou F1-F24 | Ctrl+S | tout de suite |
| Comportement | Sauvegarde auto : | Off, 10, 30, 60, 120, 300 sec | 30 sec | au démarrage suivant |
| Comportement | Réduire dans la zone de notification | oui / non | non | tout de suite |
| Comportement | Raccourci global : | affiché seulement | Ctrl+Shift+L | - |

### Fichier > Paramètres... > Sécurité...

| Option | Valeurs | Par défaut | S'applique |
|---|---|---|---|
| Changer le mot de passe... | ouvre la fenêtre de création de mot de passe (nouveau mot de passe deux fois) | - | après enregistrement |
| Long. min. mot de passe : | 1, 4, 6, 8, 12, 16 | 6 (4 pour les notes faites avec des versions qui n'avaient pas ce réglage) | à la prochaine création ou au prochain changement de mot de passe |
| Verrouillage auto : | Off, 1, 5, 10, 15, 30, 60 min | 15 min | tout de suite |
| Effacement presse-papiers : | Off, 10, 30, 60, 120 sec | 30 sec | au démarrage suivant |
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
| Ctrl+G | Aller à la ligne |
| Ctrl+D | Dupliquer la ligne |
| Ctrl+Maj+K | Supprimer la ligne |
| Ctrl+. | Ajouter ou enlever une puce sur la ligne |
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
