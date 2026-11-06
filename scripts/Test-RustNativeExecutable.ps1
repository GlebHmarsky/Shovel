# Inspects PE bytes only. Never loads or runs an executable.
function Test-RustNativeExecutable {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Version,
        [Parameter(Mandatory)][ValidateSet('ShovelRust.exe','ShovelRustAskPass.exe')][string]$Executable
    )
    $ErrorActionPreference = 'Stop'
    if ($null -eq ('ShovelRust.Packaging.NativeImage' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Text;
using System.Collections.Generic;
using System.Reflection.PortableExecutable;
namespace ShovelRust.Packaging {
    public sealed class NativeImage {
        public int Machine, Subsystem;
        public bool Managed;
        public string[] Imports;
        public uint[] ResourceTypes;
        public string Manifest;
        static string Name(PEReader pe, int address) {
            var reader = pe.GetSectionData(address).GetReader();
            var bytes = new List<byte>();
            while (reader.RemainingBytes > 0 && bytes.Count < 1024) {
                byte next = reader.ReadByte();
                if (next == 0) return Encoding.ASCII.GetString(bytes.ToArray());
                bytes.Add(next);
            }
            throw new InvalidDataException("Invalid native import name.");
        }
        static void ImportsAt(PEReader pe, DirectoryEntry entry, bool delay, List<string> names) {
            if (entry.RelativeVirtualAddress == 0) return;
            var reader = pe.GetSectionData(entry.RelativeVirtualAddress).GetReader(0, entry.Size);
            int width = delay ? 8 : 5;
            while (reader.RemainingBytes >= width * 4) {
                var row = new uint[width]; bool zero = true;
                for (int i=0; i<width; i++) { row[i]=reader.ReadUInt32(); zero &= row[i]==0; }
                if (zero) return;
                if (names.Count >= 256 || (delay && row[0] != 1)) throw new InvalidDataException("Invalid native import directory.");
                names.Add(Name(pe, checked((int)row[delay ? 1 : 3])));
            }
            throw new InvalidDataException("Truncated native import directory.");
        }
        static uint U32(byte[] data, int offset) { return BitConverter.ToUInt32(data, offset); }
        static ushort U16(byte[] data, int offset) { return BitConverter.ToUInt16(data, offset); }
        static int First(byte[] data, int offset) {
            int count = U16(data,offset+12)+U16(data,offset+14);
            if (count < 1) throw new InvalidDataException("Missing manifest resource.");
            return checked((int)(U32(data,offset+20) & 0x7fffffff));
        }
        public static NativeImage Read(string path) {
            using var stream = File.OpenRead(path);
            using var pe = new PEReader(stream);
            var header = pe.PEHeaders.PEHeader ?? throw new InvalidDataException("Missing PE header.");
            if (header.Magic != PEMagic.PE32Plus) throw new InvalidDataException("Expected PE32+.");
            var imports = new List<string>();
            ImportsAt(pe,header.ImportTableDirectory,false,imports);
            ImportsAt(pe,header.DelayImportTableDirectory,true,imports);
            var resource = pe.GetSectionData(header.ResourceTableDirectory.RelativeVirtualAddress).GetReader(0,header.ResourceTableDirectory.Size);
            byte[] data = resource.ReadBytes(resource.Length);
            var types = new List<uint>(); string manifest = null;
            int count = U16(data,12)+U16(data,14);
            for(int i=0;i<count;i++) {
                uint id = U32(data,16+i*8); types.Add(id);
                if(id != 24) continue;
                int type = checked((int)(U32(data,20+i*8)&0x7fffffff));
                int language = First(data,type);
                int leaf = First(data,language);
                int rva = checked((int)U32(data,leaf)), length = checked((int)U32(data,leaf+4));
                var text = pe.GetSectionData(rva).GetReader(0,length);
                manifest = Encoding.UTF8.GetString(text.ReadBytes(length));
            }
            return new NativeImage {
                Machine=(int)pe.PEHeaders.CoffHeader.Machine, Subsystem=(int)header.Subsystem,
                Managed=pe.PEHeaders.CorHeader != null || header.CorHeaderTableDirectory.RelativeVirtualAddress != 0,
                Imports=imports.ToArray(), ResourceTypes=types.ToArray(), Manifest=manifest
            };
        }
    }
}
'@
    }
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $image = [ShovelRust.Packaging.NativeImage]::Read($resolved)
    $subsystem = if ($Executable -eq 'ShovelRust.exe') { 2 } else { 3 }
    if ($image.Machine -ne 0x8664 -or $image.Managed -or $image.Subsystem -ne $subsystem) { throw 'Rust executable must be native x64 with the intended Windows subsystem.' }
    $forbidden = @($image.Imports | Where-Object { $_ -match '(?i)^(vcruntime|msvcp|msvcr|coreclr|hostfxr|hostpolicy|mscoree|clr)\d*.*\.dll$' })
    if ($forbidden.Count) { throw "Unexpected runtime dependency: $($forbidden -join ', '). Build Rust with static CRT linking." }
    foreach ($type in @(3,14,16,24)) { if ($type -notin $image.ResourceTypes) { throw "Missing native resource type $type." } }
    $metadata = [Diagnostics.FileVersionInfo]::GetVersionInfo($resolved)
    if ($metadata.ProductName -ne 'ShovelRust' -or $metadata.ProductVersion -ne $Version -or $metadata.FileVersion -ne "$Version.0" -or $metadata.OriginalFilename -ne $Executable) { throw 'Native executable identity/version does not match the Rust package.' }
    [xml]$manifest = $image.Manifest.TrimStart([char]0xfeff)
    if ($manifest.assembly.assemblyIdentity.name -ne [IO.Path]::GetFileNameWithoutExtension($Executable) -or
        $manifest.assembly.trustInfo.security.requestedPrivileges.requestedExecutionLevel.level -ne 'asInvoker' -or
        $manifest.assembly.application.windowsSettings.dpiAwareness.InnerText -ne 'PerMonitorV2' -or
        $manifest.assembly.application.windowsSettings.longPathAware.InnerText -ne 'true') { throw 'Native Windows manifest does not match Rust application requirements.' }
    [pscustomobject]@{ File=$Executable; Version=$Version; Architecture='win-x64'; Managed=$false; StaticCrt=$true; Subsystem=$image.Subsystem; Imports=$image.Imports; ResourceTypes=$image.ResourceTypes; SHA256=(Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant(); Size=(Get-Item -LiteralPath $resolved).Length }
}
