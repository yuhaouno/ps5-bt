# Audio Bluetooth pour PS5 — titre provisoire

Une pile audio Bluetooth conçue autour de Rust pour les environnements homebrew
PS5, visant une faible latence, une bonne qualité et la portabilité. L'objectif
est de connecter des écouteurs ordinaires au Bluetooth intégré tout en conservant
le fonctionnement sans fil de la DualSense.

**État : recherche et structure initiale de phase 0. Aucun payload audio
utilisable, aucune compatibilité ni latence vérifiée.** L'adaptateur C renvoie
explicitement « non pris en charge ». Les tests simulés ne prouvent pas le bon
fonctionnement sur PS5. Le partage du contrôleur Bluetooth avec le système et
le démarrage du SDK restent à examiner.

Ce projet indépendant n'est pas une traduction de C en Rust. Les adaptations de
plateforme en C pourront être réutilisées avec leurs licences et attributions.
Il ne fournit ni exploits, ni élévation de privilèges, ni contournement de DRM,
ni binaires Sony ou firmware. Les contributions originales sont sous
GPL-3.0-or-later.

Ce texte est un bref état des lieux. Le [README anglais](../../README.md) fait
autorité si la traduction devient obsolète. Consultez-le pour la compilation,
l'architecture, la recherche et la feuille de route. Le nom définitif sera choisi
après vérification des conflits de noms de dépôts et de paquets.
