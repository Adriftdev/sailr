import clsx from 'clsx';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import HomepageFeatures from '@site/src/components/HomepageFeatures';
import CodeBlock from '@theme/CodeBlock';
import Heading from '@theme/Heading';
import styles from './index.module.css';

function HomepageHeader() {
  const { siteConfig } = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          Sailr
        </Heading>
        <p>Kubernetes is a powerful tool for managing containerized applications, but it can also be complex and challenging to use. If you're feeling overwhelmed by Kubernetes, Sailr can help. Sailr is an environment management CLI that makes it easy to deploy, manage, and troubleshoot Kubernetes applications. </p>
        <p className="hero__subtitle">Simply Copy & Paste the following in your terminal to install Sailr Cli</p>
        <div className={styles.buttons}>


          <CodeBlock language="sh">cargo install --git https://github.com/Adriftdev/sailr</CodeBlock>
        </div>
      </div>
    </header >
  );
}

export default function Home() {
  const { siteConfig } = useDocusaurusContext();
  return (
    <Layout
      title={"Home"}
      description="Sailr is an environment management CLI that makes it easy to deploy, manage, and troubleshoot Kubernetes applications.">
      <HomepageHeader />
      <main>
        <HomepageFeatures />
      </main>
    </Layout>
  );
}
