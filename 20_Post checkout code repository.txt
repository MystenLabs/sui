2023-05-30T11:24:46.6326277Z Post job cleanup.
2023-05-30T11:24:46.7885506Z [command]/usr/bin/git version
2023-05-30T11:24:46.7947131Z git version 2.40.1
2023-05-30T11:24:46.8000850Z Copying '/home/runner/.gitconfig' to '/home/runner/work/_temp/3600a614-2b3c-466b-bb7a-6534b77d4c5a/.gitconfig'
2023-05-30T11:24:46.8017934Z Temporarily overriding HOME='/home/runner/work/_temp/3600a614-2b3c-466b-bb7a-6534b77d4c5a' before making global git config changes
2023-05-30T11:24:46.8019558Z Adding repository directory to the temporary git global config as a safe directory
2023-05-30T11:24:46.8029430Z [command]/usr/bin/git config --global --add safe.directory /home/runner/work/sui/sui
2023-05-30T11:24:46.8088014Z [command]/usr/bin/git config --local --name-only --get-regexp core\.sshCommand
2023-05-30T11:24:46.8130168Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :"
2023-05-30T11:24:46.8438764Z [command]/usr/bin/git config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
2023-05-30T11:24:46.8469231Z http.https://github.com/.extraheader
2023-05-30T11:24:46.8482821Z [command]/usr/bin/git config --local --unset-all http.https://github.com/.extraheader
2023-05-30T11:24:46.8526182Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :"
