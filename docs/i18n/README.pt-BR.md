# Áudio Bluetooth para PS5 — título provisório

Uma pilha de áudio Bluetooth com Rust como base, voltada a ambientes homebrew do
PS5 e projetada para baixa latência, qualidade e portabilidade. A meta é usar
fones comuns pelo Bluetooth integrado, mantendo o DualSense sem fio.

**Estado: pesquisa e estrutura inicial da fase 0. Ainda não há payload de áudio
utilizável nem compatibilidade ou latência verificadas.** O adaptador C retorna
explicitamente «não suportado». Testes simulados não comprovam funcionamento no
PS5. O compartilhamento do controlador Bluetooth com o sistema e a inicialização
do SDK ainda precisam ser revisados.

Este é um projeto independente, não uma tradução de C para Rust. A adaptação de
plataforma em C poderá ser reutilizada com as licenças e os créditos devidos.
Não inclui exploits, elevação de privilégios, contorno de DRM, binários da Sony
ou firmware. As contribuições originais usam GPL-3.0-or-later.

Este é um resumo do estado. O [README em inglês](../../README.md) é a referência
oficial e prevalece se a tradução estiver desatualizada. Consulte nele a
compilação, arquitetura, pesquisa e roteiro. O nome definitivo depende de uma
verificação de conflitos de nomes de repositórios e pacotes.
