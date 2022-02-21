function ExternalLink({href, label} : {href : string, label: string}) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noreferrer noopener"
    >
      {label}
    </a>
  )
}

export default ExternalLink;
