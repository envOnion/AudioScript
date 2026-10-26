import { PermissionPanel } from '@/main/permissions/PermissionPanel'
import { Page } from '@/ui/Page'

export function PermissionsPage() {
  return (
    <Page title="Разрешения">
      <PermissionPanel />
    </Page>
  )
}
