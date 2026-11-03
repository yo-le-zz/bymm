# bymm

**Stocke des données dans n'importe quel exécutable — puis relis-les, modifie-les, efface-les.**
Par [yolezz](https://github.com/yo-le-zz) · [English](README.md) · licence MIT · zéro dépendance

`bymm` ajoute à la fin d'un binaire un petit stockage clé/valeur protégé par
checksums, et le retrouve en regardant la fin du fichier. Les chargeurs
d'exécutables ignorent ce qui suit la fin de l'image : **le programme continue de
s'exécuter exactement comme avant**, sur toutes les architectures CPU, sous
Linux et Windows (et macOS).

```rust
bymm::write("./mon-app", "pseudo", "yolezz")?;            // écrit pour de vrai dans ./mon-app
let nom = bymm::read_string("./mon-app", "pseudo")?;      // Some("yolezz")
```

## Fonctionnalités

| | |
|---|---|
| **Écriture réelle dans le binaire** | Les données sont dans le fichier exécutable lui-même, pas dans un fichier à côté. |
| **Détection automatique** | ELF, PE (Windows), Mach-O (simple et universel) ; architecture, 32/64 bits, endianness, fin de l'image, signature. Les fichiers inconnus fonctionnent aussi. |
| **Toutes les architectures** | x86, x86_64, ARM, AArch64, ARM64EC, RISC-V 32/64, MIPS, PowerPC, s390x, SPARC, LoongArch, Itanium, m68k, SuperH… — le stockage ne dépend pas de l'architecture. |
| **Contrôle total** | `write`, `append`, `read`, `delete`, `clear`, `strip` (rend le binaire d'origine octet pour octet), `verify`. |
| **Limite de taille** | On la fixe (`set_limit`), on la voit (`limit`, `used`, `free`, `max_value_len`), on la retire (`set_unlimited`). Elle est enregistrée dans le binaire. |
| **Résistant aux pannes** | Chaque commit construit le nouveau binaire dans un fichier temporaire puis l'échange de façon atomique. Checksum CRC-32 sur chaque entrée. |
| **Programmes qui se modifient** | `Bymm::open_self()` — un exécutable peut lire *et réécrire* le fichier dont il est lancé (Linux et Windows). |
| **Sûr par défaut** | Refuse de casser en silence une signature numérique ; refuse d'écraser un fichier modifié entre-temps. |
| **CLI incluse** | `cargo install bymm` installe la commande `bymm`. |

## Installation

```toml
[dependencies]
bymm = "1"
```

Nécessite Rust 1.85+. CLI : `cargo install bymm`.

## Utilisation

### Un appel par action

```rust
use bymm::{write, append, read, read_string, delete, keys};

write("./mon-app", "config", br#"{"theme":"sombre"}"#)?;
append("./mon-app", "historique", "lancé\n")?;
assert_eq!(read_string("./mon-app", "config")?.as_deref(), Some(r#"{"theme":"sombre"}"#));
delete("./mon-app", "historique")?;
println!("{:?}", keys("./mon-app")?);
```

Chaque appel est atomique mais réécrit le fichier. Pour enchaîner plusieurs modifications, utilise un handle :

### Un handle : préparer plusieurs changements, valider une fois

```rust
use bymm::Bymm;

let mut b = Bymm::open("./mon-app")?;
println!("{} / {} / {} bits", b.info().format, b.info().arch(), b.info().bits.unwrap_or(0));
// ELF / x86_64 / 64 bits

b.set_limit(1024 * 1024)?;            // 1 Mio maximum pourra être embarqué
b.write("nom", "yolezz")?;
b.append("log", "salut\n")?;
b.delete("ancienne-cle")?;
b.commit()?;                          // rien ne touche le fichier avant cette ligne
```

Les changements sont *préparés en mémoire* ; les lectures voient l'état préparé ;
`b.discard()?` les jette ; `b.is_dirty()` indique si un commit est en attente.

### Limites : les voir, les fixer

```rust
let b = bymm::Bymm::open("./mon-app")?;
b.limit();                  // 67108864 (64 Mio par défaut) ; u64::MAX = illimité
b.used();                   // octets occupés par le stockage (en-tête + entrées + pied)
b.free();                   // octets qu'on peut encore ajouter
b.max_value_len("cle")?;    // plus grande valeur acceptée par write("cle", …) maintenant
b.usage();                  // tout ça dans une struct (implémente Display)
```

`set_limit(n)` échoue si `n` est plus petit que ce qui est déjà utilisé. Une
écriture qui dépasserait la limite échoue avec `Error::LimitExceeded` et ne
change rien. La limite est enregistrée dans le binaire. La valeur par défaut d'un
*nouveau* stockage se règle avec `Options::new().default_limit(…)`.

### Un programme qui stocke des données dans lui-même

```rust
fn main() -> bymm::Result<()> {
    let mut moi = bymm::Bymm::open_self()?;
    let n: u32 = moi.read_string("lancements")?.and_then(|s| s.parse().ok()).unwrap_or(0);
    println!("déjà lancé {n} fois");
    moi.write("lancements", (n + 1).to_string())?;
    moi.commit()?;              // réécrit cet exécutable lui-même, de façon atomique
    Ok(())
}
```

(Pour essayer : `cargo build --example counter`, copie `target/debug/examples/counter` ailleurs, lance la copie.)
Le processus *en cours* continue d'exécuter son ancienne image ; le lancement suivant voit les nouvelles données.

### Détecter ce qu'est un fichier

```rust
let info = bymm::see::info("./un-binaire")?;
info.format;      // Format::Elf | Pe | MachO | MachOUniversal | Unknown
info.arch();      // Arch::X86_64, Arch::Aarch64, Arch::Riscv64, …
info.bits;        // Some(64)
info.endian;      // Some(Endian::Little)
info.image_end;   // fin de l'image exécutable (le reste est un « overlay »)
info.signed;      // true si signé (Authenticode / Mach-O)
```

### Ligne de commande

```text
bymm info   <binaire>                 format, architecture, usage du stockage
bymm ls     <binaire>                 liste les clés et leur taille
bymm get    <binaire> <clé>           écrit la valeur sur la sortie standard
bymm set    <binaire> <clé> <valeur>  valeur = texte, @fichier ou - (entrée standard)
bymm append <binaire> <clé> <valeur>
bymm rm     <binaire> <clé>
bymm clear  <binaire>                 supprime toutes les entrées
bymm strip  <binaire>                 retire le stockage, restaure l'original
bymm limit  <binaire> [octets|unlimited]
bymm verify <binaire>                 vérifie tous les checksums
options : --backup (garde <binaire>.bymm.bak)   --allow-signed
```

## Comment ça marche

```text
[ exécutable d'origine ][ en-tête 32 o ][ entrée ]…[ pied 16 o ]
                                                     └─ "BYMMEND\0" + longueur : retrouvé depuis la fin du fichier
```

Ouvrir un fichier lit ses 16 derniers octets ; si c'est un pied de page, le
stockage est indexé (les valeurs ne sont *pas* chargées). Écrire construit
*octets d'origine + nouveau stockage* dans un fichier temporaire à côté du
binaire, le `fsync`, recopie les permissions puis le renomme par-dessus
l'original. C'est aussi ce qui permet à un programme en cours d'exécution de se
mettre à jour : Linux refuse d'ouvrir un exécutable en cours d'exécution en
écriture, mais accepte de remplacer son entrée de répertoire. Sous Windows, où
un `.exe` lancé ne peut pas être écrasé mais peut être renommé, l'ancien fichier
est d'abord écarté.

La spécification complète est dans [`docs/FORMAT.md`](docs/FORMAT.md).

## À savoir

* **Signatures numériques.** Modifier un binaire signé invalide sa signature
  (Authenticode, signature macOS). `bymm` renvoie `Error::SignedBinary` sauf si
  tu passes `Options::new().allow_signed(true)`. Re-signe ensuite.
* **Outils de post-traitement** (`strip`, `upx`, `objcopy`, étapes « pack » des
  installeurs) réécrivent le fichier et perdent ce qu'ils ne connaissent pas :
  embarque les données *après* eux.
* **Accès au dossier.** Un commit nécessite le droit d'écriture sur le *dossier*
  du binaire (il échange des fichiers). Les permissions sont conservées ; le
  propriétaire, les ACL et attributs étendus ne le sont pas, et les liens durs
  ne sont pas suivis.
* **Concurrence.** Deux processus qui valident en même temps ne sont pas
  fusionnés : le second reçoit `Error::Modified` au lieu d'écraser le premier.
* **Mémoire.** Les valeurs sont gardées en mémoire jusqu'au commit ; `read_to`
  lit en flux. Le commit recopie les données inchangées en flux depuis le disque.
* **Résidus.** Sous Windows, si l'image *ancienne* en cours d'exécution ne peut
  pas être supprimée tout de suite, un fichier caché `.<nom>.bymm-old` est
  supprimé au commit suivant.
* **macOS.** Mach-O est détecté et le mécanisme est identique au niveau du
  fichier, mais une app signée/notarisée doit être re-signée après écriture.

## Erreurs

Tous les appels faillibles renvoient `bymm::Result<T>`. `Error` est
`#[non_exhaustive]` : `Io`, `InvalidKey`, `LimitExceeded`, `InvalidLimit`,
`Corrupt`, `UnsupportedVersion`, `SignedBinary`, `Modified`. Elle implémente
`std::error::Error` et se convertit en `std::io::Error`.

## Tests

`cargo test` lance les tests unitaires, les doc-tests et les tests d'intégration :
31 fichiers synthétiques (ELF/PE/Mach-O, nombreuses architectures,
endianness et largeurs), de vrais binaires système, les bornes de la limite, la
détection de corruption, et un test de bout en bout qui embarque des données
dans l'exécutable `bymm` lui-même, **le lance**, et le fait se réécrire. Le CI
(`.github/workflows/ci.yml`) est prévu pour tout exécuter sur Linux x86_64, Linux ARM64, Windows
et macOS. `test/` contient un workspace séparé (`test/run_all.sh` ou
`test/run_all.ps1`) qui compile un faux exécutable et teste la bibliothèque dessus.

## Migrer depuis la 0.1.x

La 0.1.x écrivait un fichier `.bymm` indépendant via `&mut File`. La 1.0.0 écrit
**dans le binaire** et prend des chemins : `bymm::write(&mut file, clé, données)`
devient `bymm::write(chemin, clé, données)`. L'ancien format 0.1 n'est pas relu par la 1.0.

## Licence

MIT — © 2026 yolezz
