# Build the image, (re)create the VirtualBox VM if needed, and boot it.
$vb = "C:\Program Files\Oracle\VirtualBox\VBoxManage.exe"
$vm = "RingOS"
cargo run | Out-Host
$vdi = "$PSScriptRoot\target\ringos.vdi"
& $vb controlvm $vm poweroff 2>$null
Start-Sleep 4
& $vb showvminfo $vm 2>$null | Out-Null
if ($LASTEXITCODE -eq 0) { & $vb unregistervm $vm --delete 2>$null | Out-Null; Start-Sleep 1 }
Remove-Item $vdi -ErrorAction SilentlyContinue
& $vb convertfromraw "$PSScriptRoot\target\ringos.img" $vdi --format VDI | Out-Null
& $vb createvm --name $vm --ostype Other_64 --register | Out-Null
& $vb modifyvm $vm --firmware efi64 --memory 64 --vram 16 --graphicscontroller vboxvga --boot1 disk
& $vb storagectl $vm --name SATA --add sata
& $vb storageattach $vm --storagectl SATA --port 0 --device 0 --type hdd --medium $vdi
& $vb startvm $vm
