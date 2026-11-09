param([Parameter(Mandatory=$true)][string]$Path)
$ErrorActionPreference = 'Stop'
# An installer launched from pwsh can inherit incompatible PowerShell 7 modules.
# This process needs only Windows PowerShell's own modules, never user modules.
[Environment]::SetEnvironmentVariable('PSModulePath', [IO.Path]::Combine($PSHOME, 'Modules'), 'Process')
$stage = 'identity'
try {
    $identity = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
    $system = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18')
    $directory = [System.IO.DirectoryInfo]::new($Path)
    $stage = 'read_dacl'
    # Read/persist DACL only: no owner/group/SACL changes or audit privileges.
    $acl = $directory.GetAccessControl([System.Security.AccessControl.AccessControlSections]::Access)
    $stage = 'replace_dacl'
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($existing in $acl.GetAccessRules($true, $false, [System.Security.Principal.SecurityIdentifier])) {
        [void]$acl.RemoveAccessRuleSpecific($existing)
    }
    foreach ($sid in @($identity, $system)) {
        $rule = [System.Security.AccessControl.FileSystemAccessRule]::new($sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
        $acl.AddAccessRule($rule)
    }
    $stage = 'persist_dacl'
    $directory.SetAccessControl($acl)
    $stage = 'verify_dacl'
    $verified = $directory.GetAccessControl([System.Security.AccessControl.AccessControlSections]::Access)
    if (-not $verified.AreAccessRulesProtected) { throw 'Profile DACL inheritance remains enabled' }
    $rules = $verified.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier])
    if ($rules.Count -ne 2) { throw 'Unexpected profile DACL rule count' }
    foreach ($rule in $rules) {
        if (($rule.IdentityReference -ne $identity -and $rule.IdentityReference -ne $system) -or
            $rule.AccessControlType -ne [System.Security.AccessControl.AccessControlType]::Allow -or
            $rule.FileSystemRights -ne [System.Security.AccessControl.FileSystemRights]::FullControl) {
            throw 'Unexpected profile DACL rule'
        }
    }
} catch {
    $reason = $_.Exception
    while ($reason.InnerException) { $reason = $reason.InnerException }
    $type = $reason.GetType().Name
    $hresult = '0x' + $reason.HResult.ToString('X8')
    [Console]::Error.WriteLine("FAIL SSC_ACL stage=$stage type=$type hresult=$hresult")
    exit 1
}
