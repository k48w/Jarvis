# Política de Segurança (Security Policy)

## Arquitetura de Segurança do J.A.R.V.I.S. V8.0

- **Armazenamento de Credenciais**: chaves de API (ElevenLabs, Gemini) e tokens OAuth 2.0 + PKCE (ChatGPT, Google) são armazenados localmente no **Gerenciador de Credenciais do Windows** (`studio.jarvis.cinematic`).
- **Modos de Permissão**:
  - **Permissão Total (`auto`)**: execução autônoma das ferramentas locais.
  - **Revisão Automática (`review`)**: exige aprovação explícita do usuário na interface antes de executar ações que alterem arquivos ou o sistema.
  - **Somente Conversa (`safe`)**: desativa ferramentas locais de sistema e escrita em disco.
- **Proteção de Diretórios Críticos**: ferramentas de escrita (`write_file`, `edit_file`, `revert_file_change`) e terminal bloqueiam alterações em `C:\Windows`, `System32`, `Program Files` e comandos destrutivos de disco/boot.

## Reportando Vulnerabilidades

Caso encontre qualquer vulnerabilidade de segurança, abra uma issue marcada com a label `security` ou entre em contato diretamente pelo GitHub [@k48w](https://github.com/k48w).
