# Provision an Administrators-owned disposable TEMP root with protected Administrators/SYSTEM
# DACLs and named writer-test variants, set the process token default owner to Administrators,
# run `cargo nextest run --locked -p fieldbook --all-features`, and restore token state and
# delete the disposable root in `finally` (`policy-load` steps 16, 17, 19).

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$currentIdentity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($currentIdentity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "policy-tests.ps1 requires an elevated runner token with Administrators membership"
}

$tokenHelperSource = @"
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.Principal;

public static class TokenProvisioner {
    private const uint TOKEN_ADJUST_PRIVILEGES = 0x0020;
    private const uint TOKEN_ADJUST_DEFAULT = 0x0080;
    private const uint TOKEN_QUERY = 0x0008;
    private const int TokenOwner = 4;
    private const uint SE_PRIVILEGE_ENABLED = 0x00000002;

    [StructLayout(LayoutKind.Sequential)]
    private struct LUID {
        public uint LowPart;
        public int HighPart;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct LUID_AND_ATTRIBUTES {
        public LUID Luid;
        public uint Attributes;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct TOKEN_PRIVILEGES {
        public uint PrivilegeCount;
        public LUID_AND_ATTRIBUTES Privilege;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct TOKEN_OWNER {
        public IntPtr Owner;
    }

    [DllImport("kernel32.dll")]
    private static extern IntPtr GetCurrentProcess();

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool OpenProcessToken(IntPtr ProcessHandle, uint DesiredAccess, out IntPtr TokenHandle);

    [DllImport("advapi32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern bool LookupPrivilegeValue(string lpSystemName, string lpName, out LUID lpLuid);

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool AdjustTokenPrivileges(IntPtr TokenHandle, bool DisableAllPrivileges, ref TOKEN_PRIVILEGES NewState, uint BufferLength, IntPtr PreviousState, IntPtr ReturnLength);

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool GetTokenInformation(IntPtr TokenHandle, int TokenInformationClass, IntPtr TokenInformation, int TokenInformationLength, out int ReturnLength);

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool SetTokenInformation(IntPtr TokenHandle, int TokenInformationClass, ref TOKEN_OWNER TokenInformation, int TokenInformationLength);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CloseHandle(IntPtr hObject);

    public static void EnablePrivilege(string name, bool enable) {
        IntPtr hToken;
        if (!OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, out hToken)) {
            throw new Win32Exception(Marshal.GetLastWin32Error(), "OpenProcessToken failed");
        }
        try {
            LUID luid;
            if (!LookupPrivilegeValue(null, name, out luid)) {
                throw new Win32Exception(Marshal.GetLastWin32Error(), "LookupPrivilegeValue failed for " + name);
            }
            TOKEN_PRIVILEGES tp = new TOKEN_PRIVILEGES();
            tp.PrivilegeCount = 1;
            tp.Privilege.Luid = luid;
            tp.Privilege.Attributes = enable ? SE_PRIVILEGE_ENABLED : 0u;
            if (!AdjustTokenPrivileges(hToken, false, ref tp, 0, IntPtr.Zero, IntPtr.Zero)) {
                throw new Win32Exception(Marshal.GetLastWin32Error(), "AdjustTokenPrivileges failed for " + name);
            }
            int err = Marshal.GetLastWin32Error();
            if (err != 0) {
                throw new Win32Exception(err, "AdjustTokenPrivileges did not assign " + name);
            }
        } finally {
            CloseHandle(hToken);
        }
    }

    public static string GetDefaultOwnerSid() {
        IntPtr hToken;
        if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, out hToken)) {
            throw new Win32Exception(Marshal.GetLastWin32Error(), "OpenProcessToken(TOKEN_QUERY) failed");
        }
        try {
            int len = 0;
            GetTokenInformation(hToken, TokenOwner, IntPtr.Zero, 0, out len);
            IntPtr buf = Marshal.AllocHGlobal(len);
            try {
                if (!GetTokenInformation(hToken, TokenOwner, buf, len, out len)) {
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "GetTokenInformation(TokenOwner) failed");
                }
                TOKEN_OWNER to = (TOKEN_OWNER)Marshal.PtrToStructure(buf, typeof(TOKEN_OWNER));
                SecurityIdentifier sid = new SecurityIdentifier(to.Owner);
                return sid.Value;
            } finally {
                Marshal.FreeHGlobal(buf);
            }
        } finally {
            CloseHandle(hToken);
        }
    }

    public static void SetDefaultOwnerSid(string sidString) {
        IntPtr hToken;
        if (!OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_DEFAULT | TOKEN_QUERY, out hToken)) {
            throw new Win32Exception(Marshal.GetLastWin32Error(), "OpenProcessToken(TOKEN_ADJUST_DEFAULT) failed");
        }
        try {
            SecurityIdentifier sid = new SecurityIdentifier(sidString);
            byte[] sidBytes = new byte[sid.BinaryLength];
            sid.GetBinaryForm(sidBytes, 0);
            IntPtr sidPtr = Marshal.AllocHGlobal(sidBytes.Length);
            try {
                Marshal.Copy(sidBytes, 0, sidPtr, sidBytes.Length);
                TOKEN_OWNER to = new TOKEN_OWNER();
                to.Owner = sidPtr;
                if (!SetTokenInformation(hToken, TokenOwner, ref to, Marshal.SizeOf(typeof(TOKEN_OWNER)))) {
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "SetTokenInformation(TokenOwner) failed");
                }
            } finally {
                Marshal.FreeHGlobal(sidPtr);
            }
        } finally {
            CloseHandle(hToken);
        }
    }
}
"@

Add-Type -TypeDefinition $tokenHelperSource -Language CSharp

$adminsSid = New-Object System.Security.Principal.SecurityIdentifier("S-1-5-32-544")
$systemSid = New-Object System.Security.Principal.SecurityIdentifier("S-1-5-18")
$runnerSid = $currentIdentity.User

$origTmp = $env:TMP
$origTemp = $env:TEMP
$origVariants = $env:FIELDBOOK_WIN_POLICY_VARIANTS
$origOwnerSid = [TokenProvisioner]::GetDefaultOwnerSid()
$dispRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("fieldbook-policy-" + [Guid]::NewGuid().ToString("N"))

function Set-ProtectedAdminSystemAcl {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][bool]$IsDirectory,
        [System.Security.Principal.SecurityIdentifier]$OwnerSid = $adminsSid,
        [System.Security.Principal.SecurityIdentifier]$ExtraWriterSid = $null
    )
    if ($IsDirectory) {
        $acl = New-Object System.Security.AccessControl.DirectorySecurity
        $inherit = [System.Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [System.Security.AccessControl.InheritanceFlags]::ObjectInherit
        $prop = [System.Security.AccessControl.PropagationFlags]::None
        $acl.SetAccessRuleProtection($true, $false)
        $acl.SetOwner($OwnerSid)
        $acl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule($adminsSid, [System.Security.AccessControl.FileSystemRights]::FullControl, $inherit, $prop, [System.Security.AccessControl.AccessControlType]::Allow)))
        $acl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule($systemSid, [System.Security.AccessControl.FileSystemRights]::FullControl, $inherit, $prop, [System.Security.AccessControl.AccessControlType]::Allow)))
        if ($null -ne $ExtraWriterSid) {
            $acl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule($ExtraWriterSid, [System.Security.AccessControl.FileSystemRights]::Modify, $inherit, $prop, [System.Security.AccessControl.AccessControlType]::Allow)))
        }
        [System.IO.Directory]::SetAccessControl($Path, $acl)
    } else {
        $acl = New-Object System.Security.AccessControl.FileSecurity
        $acl.SetAccessRuleProtection($true, $false)
        $acl.SetOwner($OwnerSid)
        $acl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule($adminsSid, [System.Security.AccessControl.FileSystemRights]::FullControl, [System.Security.AccessControl.AccessControlType]::Allow)))
        $acl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule($systemSid, [System.Security.AccessControl.FileSystemRights]::FullControl, [System.Security.AccessControl.AccessControlType]::Allow)))
        if ($null -ne $ExtraWriterSid) {
            $acl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule($ExtraWriterSid, [System.Security.AccessControl.FileSystemRights]::Write, [System.Security.AccessControl.AccessControlType]::Allow)))
        }
        [System.IO.File]::SetAccessControl($Path, $acl)
    }
}

try {
    [TokenProvisioner]::EnablePrivilege("SeRestorePrivilege", $true)
    [TokenProvisioner]::EnablePrivilege("SeTakeOwnershipPrivilege", $true)
    [TokenProvisioner]::SetDefaultOwnerSid("S-1-5-32-544")

    [void][System.IO.Directory]::CreateDirectory($dispRoot)
    Set-ProtectedAdminSystemAcl -Path $dispRoot -IsDirectory $true

    $repoRoot = Split-Path -Parent $PSScriptRoot
    $fixturePolicy = Get-Content -LiteralPath (Join-Path $repoRoot "tests\fixtures\policy\policy.toml") -Raw

    $variantsRoot = Join-Path $dispRoot "variants"
    [void][System.IO.Directory]::CreateDirectory($variantsRoot)
    Set-ProtectedAdminSystemAcl -Path $variantsRoot -IsDirectory $true

    # 1. normal
    $normalDir = Join-Path $variantsRoot "normal"
    [void][System.IO.Directory]::CreateDirectory($normalDir)
    Set-ProtectedAdminSystemAcl -Path $normalDir -IsDirectory $true
    $normalFile = Join-Path $normalDir "policy.toml"
    [System.IO.File]::WriteAllText($normalFile, $fixturePolicy)
    Set-ProtectedAdminSystemAcl -Path $normalFile -IsDirectory $false

    # 2. file-writable
    $fwDir = Join-Path $variantsRoot "file-writable"
    [void][System.IO.Directory]::CreateDirectory($fwDir)
    Set-ProtectedAdminSystemAcl -Path $fwDir -IsDirectory $true
    $fwFile = Join-Path $fwDir "policy.toml"
    [System.IO.File]::WriteAllText($fwFile, $fixturePolicy)
    Set-ProtectedAdminSystemAcl -Path $fwFile -IsDirectory $false -ExtraWriterSid $runnerSid

    # 3. parent-writable
    $pwDir = Join-Path $variantsRoot "parent-writable"
    [void][System.IO.Directory]::CreateDirectory($pwDir)
    Set-ProtectedAdminSystemAcl -Path $pwDir -IsDirectory $true -ExtraWriterSid $runnerSid
    $pwFile = Join-Path $pwDir "policy.toml"
    [System.IO.File]::WriteAllText($pwFile, $fixturePolicy)
    Set-ProtectedAdminSystemAcl -Path $pwFile -IsDirectory $false

    # 4. wrong-owner (owner set to runner account SID instead of Administrators/SYSTEM)
    $woDir = Join-Path $variantsRoot "wrong-owner"
    [void][System.IO.Directory]::CreateDirectory($woDir)
    Set-ProtectedAdminSystemAcl -Path $woDir -IsDirectory $true
    $woFile = Join-Path $woDir "policy.toml"
    [System.IO.File]::WriteAllText($woFile, $fixturePolicy)
    Set-ProtectedAdminSystemAcl -Path $woFile -IsDirectory $false -OwnerSid $runnerSid

    $env:TMP = $dispRoot
    $env:TEMP = $dispRoot
    $env:FIELDBOOK_WIN_POLICY_VARIANTS = $variantsRoot

    Push-Location $repoRoot
    try {
        & cargo nextest run --locked -p fieldbook --all-features
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    } finally {
        Pop-Location
    }
} finally {
    $env:TMP = $origTmp
    $env:TEMP = $origTemp
    $env:FIELDBOOK_WIN_POLICY_VARIANTS = $origVariants
    try {
        [TokenProvisioner]::SetDefaultOwnerSid($origOwnerSid)
    } catch {}
    if (Test-Path -LiteralPath $dispRoot) {
        Remove-Item -LiteralPath $dispRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
