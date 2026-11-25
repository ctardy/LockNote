# LockNote

[English](README.md) | Français

**Documentation :** [Guide utilisateur](docs/fr/guide-utilisateur.md) (toutes
les fonctions de l'application, pas à pas) · [User guide](docs/en/user-guide.md)

Bloc-notes chiffré autonome pour Windows : le programme et vos notes forment un
seul `.exe`. Rien à installer, aucun compte, pas de cloud. Libre et open source
(GPL-3.0-or-later), écrit en **Rust** avec une interface Windows native
(native-windows-gui).

## Fonctions

| Fonction | Description |
|---------|-------------|
| Chiffrement | AES-256-CBC + HMAC-SHA256 (encrypt-then-MAC), clé dérivée par Argon2id (64 Mio, 3 itérations, 4 voies) ; chaque tampon en clair est effacé après usage |
| Notes en onglets | Rangez vos notes dans des onglets séparés d'un même fichier chiffré ; glisser-déposer pour les réordonner, l'onglet actif est mis en évidence, les onglets qui ne tiennent pas vont dans un menu « … », l'onglet actif et la ligne du curseur sont rétablis après verrouillage/déverrouillage |
| Coffre chiffré | Gestionnaire de secrets en arborescence par catégorie, ouvert au démarrage : un clic sur un secret le copie (indicateur « Copié : nom »), recherche par nom, création/renommage/suppression de catégories, glisser-déposer des secrets entre catégories, affichage (5 s, champ dédié), secrets sur plusieurs lignes avec un bouton Copier dans la fenêtre d'édition |
| Prompts | Prompts enregistrés, recherchés par nom et par texte, copiés en un clic |
| TOTP à deux facteurs | Mots de passe à usage unique basés sur le temps (RFC 6238), et code 2FA facultatif au déverrouillage |
| Pièces jointes | Fichiers stockés chiffrés dans la note |
| Sauvegardes de sécurité | Une sauvegarde chiffrée de toute la note est écrite avant chaque import et chaque mise à jour, et à la demande ; Fichier > Sauvegardes de sécurité les liste, en restaure une ou en crée une nouvelle |
| Sauvegarde complète | Fichier > Exporter / Importer une sauvegarde complète : tous les onglets, le coffre, les prompts, les pièces jointes et les paramètres dans un fichier `.lnbak` chiffré, versionné pour que les versions futures continuent de le lire |
| Import depuis un autre LockNote | Fichier > Importer depuis un autre LockNote... : transfère les données d'un autre exe LockNote dans celui qui tourne |
| Clés SSH | Outils > Clés SSH : génère des clés Ed25519 / ECDSA / RSA (passphrase facultative), les range dans le coffre, exporte des fichiers lisibles par le seul propriétaire, les charge dans le ssh-agent de Windows ; LockNote retire les clés qu'il a chargées au verrouillage, à la fermeture et au déverrouillage suivant |
| Générateur de mots de passe | Outils > Générateur de mots de passe, et un bouton Générer... dans la fenêtre d'un secret du coffre : longueur 8–64, majuscules / minuscules / chiffres / symboles, votre propre liste de symboles, exclusion facultative des caractères ambigus |
| Verrouillage auto, sauvegarde auto, effacement du presse-papiers | Chacun avec son délai |
| Raccourci global | `Ctrl+Shift+L` pour afficher/masquer depuis n'importe où |
| Multilingue | Anglais, français, espagnol, allemand |
| Rechercher et remplacer | Précédent/Suivant, compteur d'occurrences en direct, respect de la casse, mot entier |
| Mises à jour | Vérifiées sur les versions signées de ce dépôt : `SHA256SUMS.txt` est vérifié par une signature Ed25519 avant que toute empreinte soit acceptée |

La taille et la position de la fenêtre sont rétablies d'une session à l'autre.
Un double-clic sélectionne exactement le texte compris entre deux espaces (les
mots de passe restent entiers, rien n'est copié automatiquement).

## Comment les notes sont stockées

Les notes sont à la fin de l'exe, après un marqueur. Un programme en cours
d'exécution ne peut pas réécrire son propre fichier : chaque enregistrement va
donc dans un fichier d'attente de `%LOCALAPPDATA%\LockNote` (`<hash>.tmp`, avec
son compagnon `<hash>.base`) ; à la fermeture de LockNote, un petit programme
d'aide intégré à l'exe le recopie sur l'exe. D'ici là, ce `.tmp` est la seule
copie du dernier enregistrement : ne le supprimez jamais à la main (Fichier >
Paramètres > Fichiers temporaires les affiche). Voir `docs/SPEC.md`.

## Compilation

Nécessite Rust (stable) sous Windows. Les ressources Windows (icône, version,
manifeste) sont compilées par `rc.exe` (Windows SDK, chaîne MSVC) ou `windres`
(MinGW, chaîne GNU) ; la compilation échoue si elles ne peuvent pas l'être.

```cmd
scripts\build.cmd     REM → build\LockNote.exe
```

Le binaire principal embarque le programme d'aide à la recopie
(`assets\locknote-swap-embedded.exe`, généré, absent de git) : `build.cmd` le
compile d'abord. Pour le faire à la main :

```cmd
cargo build --release --bin locknote-swap
copy /y target\release\locknote-swap.exe assets\locknote-swap-embedded.exe
cargo build --release --bin locknote
```

## Tests

```cmd
scripts\test.cmd
```

Il prépare le programme d'aide, puis lance `cargo test` avec `LOCALAPPDATA`,
`TEMP` et `TMP` pointant vers un dossier jetable, pour qu'aucun test ne touche
au vrai `%LOCALAPPDATA%\LockNote` (il peut contenir la seule copie d'un
enregistrement). Ne lancez `cargo test` à la main qu'avec la même redirection.

## Migrer depuis LockNote 1.2.x

- **Mettez à jour toutes vos copies de LockNote.** Un LockNote 1.2.1 supprime,
  à son démarrage, les fichiers `.tmp` en attente des autres copies (de plus
  d'une minute) dans `%LOCALAPPDATA%\LockNote`, qui peuvent contenir leur
  dernier enregistrement.
- **LockNote 1.2.1 (édition gratuite)** : il se met à jour vers la 2.0 et garde
  vos notes (la 2.0 lit le format de la 1.2.1 ; un fichier 1.2.1 figé fait
  partie des tests). Limite connue : une ligne composée uniquement d'un
  en-tête de bloc LockNote (comme `[LOCKNOTE_VAULT]`) dans une note 1.2.1 est
  lue comme ce bloc par la 2.0 (voir CHANGELOG 2.0.0) ; vérifiez votre note
  avant la mise à jour.
- **LockNote Pro 1.2.x** : il ne voit pas la 2.0 (il lit un autre flux de
  versions). Migrez à la main :
  1. Dans l'ancien LockNote : Fichier > Exporter une sauvegarde complète
     (`.lnbak`), puis **fermez-le**.
  2. Placez le nouveau `LockNote.exe` à côté de l'ancien exe, ouvrez-le et
     choisissez un mot de passe.
  3. Fichier > Importer une sauvegarde complète avec le `.lnbak` de l'étape 1.
     Ou Fichier > Importer depuis un autre LockNote..., en choisissant l'ancien
     exe **par son chemin habituel** (la lettre de lecteur que vous utilisez
     toujours, pas un chemin UNC : un enregistrement en attente est lié au
     chemin). Vérifiez les onglets, le coffre, les codes TOTP, les prompts et
     les pièces jointes. Non importés : l'indice de mot de passe, l'état de
     session et les clés chargées dans ssh-agent.
  4. Avant de supprimer l'ancien exe, vérifiez dans Fichier > Paramètres >
     Fichiers temporaires qu'il ne lui reste aucun `.tmp` en attente.
  5. Ne lancez pas de LockNote 1.2.1 sur ces PC : son nettoyage supprime les
     fichiers `.tmp` de plus d'une minute dans `%LOCALAPPDATA%\LockNote`.
  6. `%LOCALAPPDATA%\LockNote\license.dat` (la licence de l'édition Pro) ne
     sert plus : supprimez-le à la main si vous le souhaitez.

## Documentation

- [docs/fr/guide-utilisateur.md](docs/fr/guide-utilisateur.md) : guide
  utilisateur, une section par fonction (français).
- [docs/en/user-guide.md](docs/en/user-guide.md) : le même guide en anglais.
- `docs/SPEC.md` : spécification complète (formats, cryptographie, recopie du
  fichier, interface), en anglais.
- `docs/release-signing-setup.md` : clés de signature des versions.
- `scripts/migrate-exe.ps1` : remplacer le programme d'un exe qui contient déjà
  des notes.
- `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md`.

## Licence

Copyright (C) 2025-2026 Christophe Tardy.

LockNote est un logiciel libre : vous pouvez le redistribuer et/ou le modifier
selon les termes de la GNU General Public License publiée par la Free Software
Foundation, version 3 de la licence ou (à votre choix) toute version
ultérieure. Il est distribué dans l'espoir qu'il sera utile, mais SANS AUCUNE
GARANTIE, sans même la garantie implicite de QUALITÉ MARCHANDE ou d'ADÉQUATION
À UN USAGE PARTICULIER. Voir [LICENSE](LICENSE) (en anglais) pour le texte
complet.
